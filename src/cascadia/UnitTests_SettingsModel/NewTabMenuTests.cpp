// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#include "pch.h"

#include "../TerminalSettingsModel/NewTabMenuEntry.h"
#include "../TerminalSettingsModel/FolderEntry.h"
#include "../TerminalSettingsModel/CascadiaSettings.h"
#include "../TerminalSettingsModel/resource.h"
#include "../types/inc/colorTable.hpp"
#include "JsonTestClass.h"

using namespace Microsoft::Console;
using namespace winrt::Microsoft::Terminal;
using namespace winrt::Microsoft::Terminal::Settings::Model::implementation;
using namespace WEX::Logging;
using namespace WEX::TestExecution;
using namespace WEX::Common;

namespace SettingsModelUnitTests
{
    namespace Model = winrt::Microsoft::Terminal::Settings::Model;

    static constexpr std::string_view profileDefaults{ R"json({
        "defaultProfile": "{00000000-0000-0000-0000-000000000002}",
        "profiles": [
            {
                "guid": "{00000000-0000-0000-0000-000000000001}",
                "name": "Renamed agent",
                "source": "IntelligentTerminal.AgentProfiles",
                "agentProfile.id": "claude"
            },
            {
                "guid": "{00000000-0000-0000-0000-000000000002}",
                "name": "Claude",
                "commandline": "cmd.exe"
            },
            {
                "guid": "{00000000-0000-0000-0000-000000000003}",
                "name": "Copilot",
                "source": "IntelligentTerminal.AgentProfiles",
                "agentProfile.id": "copilot"
            },
            {
                "guid": "{00000000-0000-0000-0000-000000000004}",
                "name": "Agent copy",
                "agentProfile.id": "claude"
            }
        ]
    })json" };

    class NewTabMenuTests : public JsonTestClass
    {
        TEST_CLASS(NewTabMenuTests);

        TEST_METHOD(DefaultsToRemainingProfiles);
        TEST_METHOD(ParseEmptyFolder);
        TEST_METHOD(DefaultMenuGroupsAgentProfiles);
        TEST_METHOD(DefaultMenuOmitsEmptyGroups);
        TEST_METHOD(DefaultMenuExcludesHiddenAndOrphanedAgents);
        TEST_METHOD(CustomMenusAreAugmentedOnceWithoutDuplicates);
        TEST_METHOD(EditedMenuSurvivesReload);
        TEST_METHOD(NewShellsRemainDynamicWithoutAppendingNewAgents);
        TEST_METHOD(UnsavedInitializationCanRetry);
    };

    void NewTabMenuTests::DefaultMenuGroupsAgentProfiles()
    {
        for (const std::string_view user : { R"({})", R"({"newTabMenu":[{"type":"remainingProfiles"}]})" })
        {
            const auto settings = winrt::make_self<CascadiaSettings>(user, profileDefaults);
            VERIFY_IS_TRUE(settings->InitializeNewTabMenu());
            const auto entries = settings->GlobalSettings().NewTabMenu();
            VERIFY_ARE_EQUAL(4u, entries.Size());
            VERIFY_ARE_EQUAL(Model::NewTabMenuEntryType::RemainingProfiles, entries.GetAt(0).Type());
            VERIFY_ARE_EQUAL(Model::NewTabMenuEntryType::Separator, entries.GetAt(1).Type());
            const auto ordinary = entries.GetAt(0).as<Model::RemainingProfilesEntry>().Profiles();
            VERIFY_ARE_EQUAL(2u, ordinary.Size());
            VERIFY_IS_TRUE(ordinary.HasKey(1));
            VERIFY_IS_TRUE(ordinary.HasKey(3));
            VERIFY_ARE_EQUAL(0, entries.GetAt(2).as<Model::ProfileEntry>().ProfileIndex());
            VERIFY_ARE_EQUAL(2, entries.GetAt(3).as<Model::ProfileEntry>().ProfileIndex());
            const auto saved = settings->ToJson();
            VERIFY_ARE_EQUAL("remainingProfiles", saved["newTabMenu"][0]["type"].asString());
            VERIFY_ARE_EQUAL("separator", saved["newTabMenu"][1]["type"].asString());
            VERIFY_ARE_EQUAL("{00000000-0000-0000-0000-000000000001}", saved["newTabMenu"][2]["profile"].asString());
            VERIFY_ARE_EQUAL("{00000000-0000-0000-0000-000000000003}", saved["newTabMenu"][3]["profile"].asString());
            VERIFY_IS_TRUE(saved["agentProfileMenuInitialized"].asBool());
            VERIFY_ARE_EQUAL(4u, saved["profiles"]["list"].size());
            const auto json = toString(saved);
            const auto reloaded = winrt::make_self<CascadiaSettings>(std::string_view{ json }, profileDefaults);
            VERIFY_IS_FALSE(reloaded->InitializeNewTabMenu());
            VERIFY_IS_TRUE(saved["newTabMenu"] == reloaded->ToJson()["newTabMenu"]);
        }
    }

    void NewTabMenuTests::DefaultMenuOmitsEmptyGroups()
    {
        const auto noAgents = winrt::make_self<CascadiaSettings>(
            R"({"profiles":[{"guid":"{00000000-0000-0000-0000-000000000001}","hidden":true},{"guid":"{00000000-0000-0000-0000-000000000003}","hidden":true}]})",
            profileDefaults);
        VERIFY_IS_FALSE(noAgents->InitializeNewTabMenu());
        VERIFY_IS_FALSE(noAgents->ToJson()["agentProfileMenuInitialized"].asBool());
        VERIFY_ARE_EQUAL(1u, noAgents->GlobalSettings().NewTabMenu().Size());

        const auto onlyAgents = winrt::make_self<CascadiaSettings>(
            R"({"profiles":[{"guid":"{00000000-0000-0000-0000-000000000002}","hidden":true},{"guid":"{00000000-0000-0000-0000-000000000004}","hidden":true}]})",
            profileDefaults);
        VERIFY_IS_TRUE(onlyAgents->InitializeNewTabMenu());
        const auto entries = onlyAgents->GlobalSettings().NewTabMenu();
        VERIFY_ARE_EQUAL(3u, entries.Size());
        VERIFY_ARE_EQUAL(0u, entries.GetAt(0).as<Model::RemainingProfilesEntry>().Profiles().Size());
        VERIFY_ARE_EQUAL(Model::NewTabMenuEntryType::Profile, entries.GetAt(1).Type());
        VERIFY_ARE_EQUAL(Model::NewTabMenuEntryType::Profile, entries.GetAt(2).Type());
    }

    void NewTabMenuTests::DefaultMenuExcludesHiddenAndOrphanedAgents()
    {
        static constexpr std::string_view user{ R"json({
            "profiles": [
                {
                    "guid": "{00000000-0000-0000-0000-000000000001}",
                    "source": "IntelligentTerminal.AgentProfiles",
                    "hidden": true
                },
                {
                    "guid": "{00000000-0000-0000-0000-000000000005}",
                    "name": "Uninstalled agent",
                    "source": "IntelligentTerminal.AgentProfiles"
                }
            ]
        })json" };
        const auto settings = winrt::make_self<CascadiaSettings>(user, profileDefaults);
        VERIFY_IS_TRUE(settings->InitializeNewTabMenu());
        const auto entries = settings->GlobalSettings().NewTabMenu();
        VERIFY_ARE_EQUAL(3u, entries.Size());
        VERIFY_ARE_EQUAL(2u, entries.GetAt(0).as<Model::RemainingProfilesEntry>().Profiles().Size());
        VERIFY_ARE_EQUAL(winrt::hstring{ L"copilot" }, entries.GetAt(2).as<Model::ProfileEntry>().Profile().AgentProfileId());
    }

    void NewTabMenuTests::CustomMenusAreAugmentedOnceWithoutDuplicates()
    {
        for (const auto& [user, profiles, separators] : std::initializer_list<std::tuple<std::string_view, uint32_t, uint32_t>>{
                 { R"({"newTabMenu":[]})", 2, 0 },
                 { R"({"newTabMenu":[{"type":"separator"}]})", 2, 0 },
                 { R"({"newTabMenu":[{"type":"folder"}]})", 2, 0 },
                 { R"({"newTabMenu":[{"type":"profile","profile":"{00000000-0000-0000-0000-000000000001}"}]})", 1, 1 },
                 { R"({"newTabMenu":[{"type":"folder","name":"Agents","entries":[{"type":"folder","name":"Claude","entries":[{"type":"profile","profile":"{00000000-0000-0000-0000-000000000001}"}]}]}]})", 1, 1 },
                 { R"({"newTabMenu":[{"type":"matchProfiles","source":"IntelligentTerminal\\.AgentProfiles"}]})", 0, 0 },
                 { R"({"newTabMenu":[{"type":"folder","name":"All","entries":[{"type":"remainingProfiles"}]}]})", 2, 1 },
                 { R"({"newTabMenu":[{"type":"separator"},{"type":"remainingProfiles"}]})", 2, 1 },
                 { R"({"newTabMenu":[{"type":"profile","profile":"Claude"},{"type":"remainingProfiles"}]})", 2, 1 },
                 { R"({"newTabMenu":[{"type":"remainingProfiles"},{"type":"remainingProfiles"}]})", 2, 1 },
                 { R"({"newTabMenu":[{"type":"remainingProfiles"},{"type":"separator"}]})", 2, 0 } })
        {
            const auto settings = winrt::make_self<CascadiaSettings>(user, profileDefaults);
            const auto before = settings->ToJson()["newTabMenu"];
            const auto warningCount = settings->Warnings().Size();
            VERIFY_IS_TRUE(settings->InitializeNewTabMenu());
            VERIFY_ARE_EQUAL(warningCount, settings->Warnings().Size());
            const auto after = settings->ToJson()["newTabMenu"];
            VERIFY_ARE_EQUAL(before.size() + profiles + separators, after.size());
            for (Json::ArrayIndex index = 0; index < before.size(); ++index)
            {
                VERIFY_IS_TRUE(before[index] == after[index]);
            }
            if (separators)
            {
                VERIFY_ARE_EQUAL("separator", after[before.size()]["type"].asString());
            }
            for (Json::ArrayIndex index = before.size() + separators; index < after.size(); ++index)
            {
                VERIFY_ARE_EQUAL("profile", after[index]["type"].asString());
            }
            const auto saved = toString(settings->ToJson());
            const auto reloaded = winrt::make_self<CascadiaSettings>(std::string_view{ saved }, profileDefaults);
            VERIFY_IS_FALSE(reloaded->InitializeNewTabMenu());
            VERIFY_IS_TRUE(after == reloaded->ToJson()["newTabMenu"]);
        }
    }

    void NewTabMenuTests::EditedMenuSurvivesReload()
    {
        const auto settings = winrt::make_self<CascadiaSettings>(R"({})", profileDefaults);
        VERIFY_IS_TRUE(settings->InitializeNewTabMenu());
        const auto entries = settings->GlobalSettings().NewTabMenu();
        entries.RemoveAt(2);
        entries.RemoveAt(1);
        const auto copilot = entries.GetAt(1);
        entries.RemoveAt(1);
        entries.InsertAt(0, copilot);

        for (uint32_t remaining = 2;; --remaining)
        {
            const auto saved = settings->ToJson();
            const auto json = toString(saved);
            const auto reloaded = winrt::make_self<CascadiaSettings>(std::string_view{ json }, profileDefaults);
            VERIFY_IS_FALSE(reloaded->InitializeNewTabMenu());
            VERIFY_IS_TRUE(saved["newTabMenu"] == reloaded->ToJson()["newTabMenu"]);
            VERIFY_ARE_EQUAL(remaining, reloaded->GlobalSettings().NewTabMenu().Size());
            VERIFY_ARE_EQUAL(4u, reloaded->AllProfiles().Size());
            if (remaining)
            {
                const auto ordinary = reloaded->GlobalSettings().NewTabMenu().GetAt(remaining - 1).as<Model::RemainingProfilesEntry>().Profiles();
                VERIFY_ARE_EQUAL(2u, ordinary.Size());
                VERIFY_IS_FALSE(ordinary.HasKey(0));
                VERIFY_IS_FALSE(ordinary.HasKey(2));
            }
            const auto copied = reloaded->Copy();
            const auto copy = winrt::get_self<CascadiaSettings>(copied);
            VERIFY_IS_FALSE(copy->InitializeNewTabMenu());
            VERIFY_IS_TRUE(saved["newTabMenu"] == copy->ToJson()["newTabMenu"]);
            if (!remaining)
            {
                break;
            }
            entries.RemoveAt(0);
        }
    }

    void NewTabMenuTests::NewShellsRemainDynamicWithoutAppendingNewAgents()
    {
        const auto settings = winrt::make_self<CascadiaSettings>(R"({})", profileDefaults);
        VERIFY_IS_TRUE(settings->InitializeNewTabMenu());
        const auto saved = settings->ToJson();
        auto expandedDefaults = VerifyParseSucceeded(profileDefaults);
        expandedDefaults["profiles"].append(VerifyParseSucceeded(
            R"({"guid":"{00000000-0000-0000-0000-000000000005}","name":"New shell","commandline":"cmd.exe"})"));
        expandedDefaults["profiles"].append(VerifyParseSucceeded(
            R"({"guid":"{00000000-0000-0000-0000-000000000006}","name":"Codex","source":"IntelligentTerminal.AgentProfiles"})"));
        const auto defaults = toString(expandedDefaults);
        const auto user = toString(saved);
        const auto reloaded = winrt::make_self<CascadiaSettings>(std::string_view{ user }, std::string_view{ defaults });
        VERIFY_IS_FALSE(reloaded->InitializeNewTabMenu());
        VERIFY_IS_TRUE(saved["newTabMenu"] == reloaded->ToJson()["newTabMenu"]);
        const auto ordinary = reloaded->GlobalSettings().NewTabMenu().GetAt(0).as<Model::RemainingProfilesEntry>().Profiles();
        VERIFY_ARE_EQUAL(3u, ordinary.Size());
        VERIFY_ARE_EQUAL(6u, reloaded->ActiveProfiles().Size());
        for (const auto& [index, profile] : ordinary)
        {
            VERIFY_ARE_NOT_EQUAL(winrt::hstring{ L"IntelligentTerminal.AgentProfiles" }, profile.Source());
        }
    }

    void NewTabMenuTests::UnsavedInitializationCanRetry()
    {
        const auto settings = winrt::make_self<CascadiaSettings>(R"({})", profileDefaults);
        const auto original = toString(settings->ToJson());
        VERIFY_IS_TRUE(settings->InitializeNewTabMenu());
        const auto expected = settings->ToJson();
        const auto retry = winrt::make_self<CascadiaSettings>(std::string_view{ original }, profileDefaults);
        VERIFY_IS_TRUE(retry->InitializeNewTabMenu());
        VERIFY_IS_TRUE(expected["newTabMenu"] == retry->ToJson()["newTabMenu"]);
        VERIFY_IS_TRUE(retry->ToJson()["agentProfileMenuInitialized"].asBool());
    }

    void NewTabMenuTests::DefaultsToRemainingProfiles()
    {
        Log::Comment(L"If the user doesn't customize the menu, put one entry for each profile");

        static constexpr std::string_view settingsString{ R"json({
        })json" };

        try
        {
            const auto settings{ winrt::make_self<CascadiaSettings>(settingsString, LoadStringResource(IDR_DEFAULTS)) };

            VERIFY_ARE_EQUAL(0u, settings->Warnings().Size());

            const auto& entries = settings->GlobalSettings().NewTabMenu();
            VERIFY_ARE_EQUAL(1u, entries.Size());
            VERIFY_ARE_EQUAL(winrt::Microsoft::Terminal::Settings::Model::NewTabMenuEntryType::RemainingProfiles, entries.GetAt(0).Type());
        }
        catch (const SettingsException& ex)
        {
            auto loadError = ex.Error();
            loadError;
            throw ex;
        }
        catch (const SettingsTypedDeserializationException& e)
        {
            auto deserializationErrorMessage = til::u8u16(e.what());
            Log::Comment(NoThrowString().Format(deserializationErrorMessage.c_str()));
            throw e;
        }
    }

    void NewTabMenuTests::ParseEmptyFolder()
    {
        Log::Comment(L"GH #14557 - An empty folder entry shouldn't crash");

        static constexpr std::string_view settingsString{ R"json({
            "newTabMenu": [
                { "type": "folder" }
            ]
        })json" };

        try
        {
            const auto settings{ winrt::make_self<CascadiaSettings>(settingsString, LoadStringResource(IDR_DEFAULTS)) };

            VERIFY_ARE_EQUAL(0u, settings->Warnings().Size());

            const auto& entries = settings->GlobalSettings().NewTabMenu();
            VERIFY_ARE_EQUAL(1u, entries.Size());
        }
        catch (const SettingsException& ex)
        {
            auto loadError = ex.Error();
            loadError;
            throw ex;
        }
        catch (const SettingsTypedDeserializationException& e)
        {
            auto deserializationErrorMessage = til::u8u16(e.what());
            Log::Comment(NoThrowString().Format(deserializationErrorMessage.c_str()));
            throw e;
        }
    }
}
