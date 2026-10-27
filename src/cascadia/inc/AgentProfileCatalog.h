// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#pragma once

#include "AgentPaneBackend.h"
#include "AgentProfileUtils.h"
#include "AcpModelUtils.h"

namespace Microsoft::Terminal::AgentProfiles
{
    template<typename Profile>
    bool HasValidSource(const Profile& profile)
    {
        return (profile.AgentProfileSource() == L"host" && profile.AgentProfileWslDistro().empty()) ||
               (profile.AgentProfileSource() == L"wsl" &&
                WslDistroName::IsSafe(std::wstring_view{ profile.AgentProfileWslDistro() }));
    }

    template<typename Profile>
    bool IsAcpCandidate(const Profile& profile)
    {
        namespace Registry = Settings::Model::AgentRegistry;
        const std::wstring_view id{ profile.AgentProfileId() };
        if (id.empty() || profile.Deleted() || !profile.AgentProfileAcpEnabled() || !HasValidSource(profile))
            return false;
        if (!profile.AgentProfileAcpCommand().empty())
        {
            try
            {
                CustomAgents::Parse(profile.AgentProfileAcpCommand());
            }
            catch (...)
            {
                LOG_CAUGHT_EXCEPTION_MSG("Invalid ACP invocation in agent profile");
                return false;
            }
        }
        if (CustomAgents::IsId(id))
            return Settings::Model::AgentPolicy::IsCustomAgentAllowed() && !profile.AgentProfileAcpCommand().empty();
        return Settings::Model::AgentPolicy::IsAgentAllowed(id) &&
               std::ranges::any_of(Registry::BuiltinAcpAgents, [&](const auto& agent) { return Registry::AgentIdEquals(agent.id, id); });
    }

    template<typename Settings>
    auto Find(const Settings& settings, const std::wstring_view value)
    {
        using Profile = decltype(settings.AllProfiles().GetAt(0));
        for (const auto& profile : settings.AllProfiles())
        {
            if (!profile.Deleted() && til::equals_insensitive_ascii(std::wstring_view{ winrt::to_hstring(profile.Guid()) }, value))
                return profile;
        }
        return Profile{ nullptr };
    }

    template<typename Profile>
    std::wstring Backend(const Profile& profile)
    {
        return profile.AgentProfileSource() == L"wsl" ?
                   Settings::Model::AgentPaneBackend::Wsl(profile.AgentProfileWslDistro(), profile.AgentProfileId()) :
                   Settings::Model::AgentPaneBackend::Host(profile.AgentProfileId());
    }

    template<typename Profile>
    CustomAgents::Invocation AcpInvocation(const Profile& profile)
    {
        if (!profile.AgentProfileAcpCommand().empty())
            return CustomAgents::Parse(profile.AgentProfileAcpCommand());
        auto invocation = CustomAgents::Parse(AcpModels::BuildAgentCommandLine(
            profile.AgentProfileId(), std::nullopt, profile.AgentProfileSource() == L"wsl"));
        if ((profile.AgentProfileId() == L"copilot" || profile.AgentProfileId() == L"gemini") &&
            !profile.AgentProfileAcpModel().empty() &&
            !CustomModels::IsCustomSelection(std::wstring_view{ profile.AgentProfileAcpModel() }))
        {
            invocation.args.emplace_back(L"--model");
            invocation.args.emplace_back(profile.AgentProfileAcpModel());
        }
        return invocation;
    }

    template<typename Profile>
    std::wstring AcpCommand(const Profile& profile)
    {
        THROW_HR_IF_MSG(E_INVALIDARG, !IsAcpCandidate(profile), "The selected agent profile is unavailable for the agent pane");
        return CustomAgents::Format(AcpInvocation(profile));
    }

    template<typename Settings, typename Profile>
    auto Selected(const Settings& settings, const Profile& source)
    {
        const auto reference = source && !source.AgentPaneProfile().empty() ?
                                   source.AgentPaneProfile() :
                                   settings.GlobalSettings().AgentPaneDefaultProfile();
        return Find(settings, std::wstring_view{ reference });
    }

    template<typename Profiles>
    Json::Value ToJson(const Profiles& profiles)
    {
        Json::Value entries{ Json::arrayValue };
        for (const auto& profile : profiles)
        {
            if (profile.AgentProfileId().empty() || profile.Deleted() || !HasValidSource(profile))
                continue;
            const auto id = profile.AgentProfileId();
            if (!CustomAgents::IsId(id) &&
                !std::ranges::any_of(Settings::Model::AgentRegistry::BuiltinAcpAgents, [&](const auto& agent) {
                    return Settings::Model::AgentRegistry::AgentIdEquals(agent.id, id);
                }))
            {
                LOG_HR_MSG(E_INVALIDARG, "Excluding an agent profile with an unknown provider from the runtime catalog");
                continue;
            }
            Json::Value entry{ Json::objectValue };
            entry["profileId"] = winrt::to_string(winrt::to_hstring(profile.Guid()));
            entry["id"] = winrt::to_string(profile.AgentProfileId());
            entry["name"] = winrt::to_string(profile.Name());
            entry["source"] = winrt::to_string(profile.AgentProfileSource());
            entry["wslDistro"] = winrt::to_string(profile.AgentProfileWslDistro());
            entry["acpEnabled"] = IsAcpCandidate(profile);
            entry["model"] = winrt::to_string(profile.AgentProfileAcpModel());
            try
            {
                if (!profile.AgentProfileAcpCommand().empty() || !CustomAgents::IsId(profile.AgentProfileId()))
                    entry["acp"] = CustomAgents::ToJson(AcpInvocation(profile));
                if (!profile.AgentProfileCliCommand().empty())
                    entry["cli"] = CustomAgents::ToJson(CustomAgents::Parse(profile.AgentProfileCliCommand()));
                if (!profile.AgentProfileResumeCommand().empty())
                {
                    entry["resume"] = CustomAgents::ToJson(CustomAgents::Parse(profile.AgentProfileResumeCommand(), true));
                    entry["resume"]["canResumeAcpSessions"] = profile.AgentProfileSharedSessions();
                }
            }
            catch (...)
            {
                LOG_CAUGHT_EXCEPTION_MSG("Invalid agent profile invocation; excluding its capabilities from the runtime catalog");
                entry["acpEnabled"] = false;
                entry.removeMember("acp");
                entry.removeMember("cli");
                entry.removeMember("resume");
            }
            entries.append(std::move(entry));
        }
        return entries;
    }
}
