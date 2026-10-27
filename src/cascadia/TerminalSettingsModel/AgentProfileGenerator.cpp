// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#include "pch.h"
#include "AgentProfileGenerator.h"
#include "DynamicProfileUtils.h"
#include "../inc/AgentProfileUtils.h"
#include "../inc/AgentRegistry.h"
#include "../inc/CustomAgentUtils.h"
#include "../inc/AcpModelUtils.h"
#include "../inc/WtaProcess.h"
#include "../inc/WslDistroName.h"
#include "WslDistroGenerator.h"
#include <mutex>

using namespace winrt::Microsoft::Terminal::Settings::Model;
namespace AgentProfiles = ::Microsoft::Terminal::AgentProfiles;

std::wstring_view AgentProfileGenerator::GetNamespace() const noexcept
{
    return AgentProfiles::Source;
}

std::wstring_view AgentProfileGenerator::GetDisplayName() const noexcept
{
    return RS_(L"AgentProfileGeneratorDisplayName");
}

std::wstring_view AgentProfileGenerator::GetIcon() const noexcept
{
    return L"\uE99A";
}

const AgentProfileGenerator::DiscoveryCache::Snapshot& AgentProfileGenerator::DiscoveryCache::Get(
    const std::chrono::steady_clock::time_point now,
    const std::function<Snapshot()>& probe)
{
    if (!_snapshot || now - _lastProbe >= std::chrono::seconds{ 30 })
    {
        try
        {
            _snapshot = probe();
            _lastProbe = now;
        }
        catch (...)
        {
            LOG_CAUGHT_EXCEPTION_MSG("Native agent profile discovery failed; retaining the last successful snapshot");
            if (!_snapshot)
            {
                throw;
            }
        }
    }
    return *_snapshot;
}

void AgentProfileGenerator::GenerateProfiles(std::vector<winrt::com_ptr<implementation::Profile>>& profiles) const
{
    static std::mutex mutex;
    static DiscoveryCache cache;
    std::scoped_lock lock{ mutex };
    const auto& snapshot = cache.Get(std::chrono::steady_clock::now(), [] {
        return AgentProfiles::Discover(AgentProfiles::NativePath());
    });
    namespace Registry = ::Microsoft::Terminal::Settings::Model::AgentRegistry;
    for (const auto& agent : Registry::BuiltinNativeProfileAgents)
    {
        const auto found = snapshot.find(std::wstring{ agent.id });
        if (found == snapshot.end() || !::Microsoft::Terminal::Settings::Model::AgentPolicy::IsAgentAllowed(agent.id))
        {
            continue;
        }
        const auto identity = std::wstring{ AgentProfiles::Source } + L":" + std::wstring{ agent.id };
        auto profile = CreateDynamicProfile(identity);
        profile->Name(winrt::hstring{ agent.displayName });
        profile->Icon(winrt::hstring{ L"ms-appx:///AgentIcons/" + std::wstring{ agent.id } + L".svg" });
        profile->AgentProfileId(winrt::hstring{ agent.id });
        profile->AgentProfileAcpEnabled(true);
        if (Registry::AgentIdEquals(agent.id, _legacySelectedId))
            profile->AgentProfileAcpModel(_legacyModel);
        profile->Commandline(winrt::hstring{ AgentProfiles::BuildCommand(found->second.native(), agent.id, {}, {}, {}) });
        profiles.emplace_back(std::move(profile));
    }
    if (_customAgents && ::Microsoft::Terminal::Settings::Model::AgentPolicy::IsCustomAgentAllowed())
    {
        for (const auto& agent : _customAgents)
        {
            auto canonicalId = std::wstring{ agent.Id() };
            std::ranges::transform(canonicalId, canonicalId.begin(), [](const wchar_t ch) -> wchar_t { return ch >= L'A' && ch <= L'Z' ? static_cast<wchar_t>(ch + (L'a' - L'A')) : ch; });
            const auto identity = std::wstring{ AgentProfiles::Source } + L":" + canonicalId;
            auto profile = CreateDynamicProfile(identity);
            profile->Name(agent.Name());
            profile->Icon(L"\uE99A");
            profile->AgentProfileId(agent.Id());
            profile->AgentProfileCliCommand(agent.CliCommand());
            profile->AgentProfileResumeCommand(agent.ResumeCommand());
            profile->AgentProfileSharedSessions(agent.CanResumeAcpSessions());
            profile->AgentProfileAcpCommand(agent.AcpCommand());
            profile->AgentProfileAcpEnabled(!agent.AcpCommand().empty());
            if (Registry::AgentIdEquals(agent.Id(), _legacySelectedId))
                profile->AgentProfileAcpModel(_legacyModel);
            if (agent.CliCommand().empty())
            {
                profile->Hidden(true);
            }
            else
            {
                profile->Commandline(winrt::hstring{ ::Microsoft::Terminal::CustomAgents::Format(
                    ::Microsoft::Terminal::CustomAgents::Parse(agent.CliCommand())) });
            }
            profiles.emplace_back(std::move(profile));
        }
    }

    if (::Microsoft::Terminal::Settings::Model::AgentPolicy::IsCustomAgentAllowed())
    {
        std::vector<winrt::hstring> commands;
        if (!_legacySelectedCommand.empty())
            commands.emplace_back(_legacySelectedCommand);
        if (_legacyAcpCommands)
            for (const auto& command : _legacyAcpCommands)
                if (std::ranges::find(commands, command) == commands.end())
                    commands.emplace_back(command);
        for (const auto& command : commands)
        {
            if (command.empty())
                continue;
            const auto id = command == _legacySelectedCommand && ::Microsoft::Terminal::CustomAgents::IsId(_legacySelectedId) ?
                                _legacySelectedId :
                                winrt::hstring{ L"custom:" + std::wstring{ ::Microsoft::Terminal::Settings::Model::DeriveCustomAgentId(command) } };
            if (_customAgents && std::ranges::any_of(_customAgents, [&](const auto& agent) {
                    return Registry::AgentIdEquals(agent.Id(), id);
                }))
                continue;
            auto profile = CreateDynamicProfile(std::wstring{ AgentProfiles::Source } + L":legacy:" + std::wstring{ command });
            profile->Name(id);
            profile->AgentProfileId(id);
            profile->AgentProfileAcpCommand(command);
            profile->AgentProfileAcpEnabled(true);
            if (Registry::AgentIdEquals(id, _legacySelectedId))
                profile->AgentProfileAcpModel(_legacyModel);
            profile->Hidden(true);
            profile->Icon(L"\uE99A");
            profiles.emplace_back(std::move(profile));
        }
    }

    // Probe only executable availability, never initialize an ACP session.
    static std::chrono::steady_clock::time_point lastWslProbe{};
    static std::vector<winrt::com_ptr<implementation::Profile>> wslProfiles;
    const auto now = std::chrono::steady_clock::now();
    if (now - lastWslProbe >= std::chrono::seconds{ 30 })
    {
        std::vector<winrt::com_ptr<implementation::Profile>> discovered;
        const auto wta = ::Microsoft::Terminal::WtaProcess::ResolveWtaExePath();
        if (!wta.empty())
        {
            std::vector<winrt::com_ptr<implementation::Profile>> distros;
            WslDistroGenerator{}.GenerateProfiles(distros);
            for (const auto& distro : distros)
            {
                const auto name = distro->Name();
                if (!::Microsoft::Terminal::WslDistroName::IsSafe(std::wstring_view{ name }))
                    continue;
                const auto retainPrevious = [&]() {
                    for (const auto& previous : wslProfiles)
                        if (previous->AgentProfileWslDistro() == name)
                            discovered.emplace_back(previous->CopySettings());
                };
                const auto arguments = ::Microsoft::Terminal::CustomAgents::Format(
                    { L"probe-agent-sources", { L"--wsl-distro", std::wstring{ name }, L"--native-cli" } });
                const auto output = ::Microsoft::Terminal::WtaProcess::RunWtaCaptureStdout(wta, arguments, 35'000);
                if (output.empty())
                {
                    LOG_HR_MSG(E_FAIL, "WSL agent profile discovery returned no availability data");
                    retainPrevious();
                    continue;
                }
                Json::Value root;
                std::string errors;
                const std::unique_ptr<Json::CharReader> reader{ Json::CharReaderBuilder{}.newCharReader() };
                if (!reader->parse(output.data(), output.data() + output.size(), &root, &errors) || !root["agents"].isArray())
                {
                    LOG_HR_MSG(E_INVALIDARG, "WSL agent profile discovery returned invalid availability data");
                    retainPrevious();
                    continue;
                }
                for (const auto& entry : root["agents"])
                {
                    if (!entry["id"].isString())
                        continue;
                    const auto id = winrt::to_hstring(entry["id"].asString());
                    const auto agent = std::ranges::find_if(Registry::BuiltinNativeProfileAgents, [&](const auto& candidate) {
                        return Registry::AgentIdEquals(candidate.id, id);
                    });
                    if (agent == Registry::BuiltinNativeProfileAgents.end() ||
                        !::Microsoft::Terminal::Settings::Model::AgentPolicy::IsAgentAllowed(id))
                        continue;
                    auto profile = CreateDynamicProfile(std::wstring{ AgentProfiles::Source } + L":wsl:" + std::wstring{ name } + L":" + std::wstring{ id });
                    profile->Name(winrt::hstring{ std::wstring{ agent->displayName } + L" (" + std::wstring{ name } + L")" });
                    profile->AgentProfileId(id);
                    profile->AgentProfileSource(L"wsl");
                    profile->AgentProfileWslDistro(name);
                    profile->AgentProfileAcpEnabled(true);
                    profile->StartingDirectory(distro->StartingDirectory());
                    profile->Icon(winrt::hstring{ L"ms-appx:///AgentIcons/" + std::wstring{ id } + L".svg" });
                    profile->Commandline(winrt::hstring{ AgentProfiles::WslCommand(
                        { std::wstring{ Registry::CliExecutable(id) }, {} }, name) });
                    discovered.emplace_back(std::move(profile));
                }
            }
            wslProfiles = std::move(discovered);
        }
        lastWslProbe = now;
    }
    for (const auto& profile : wslProfiles)
        profiles.emplace_back(profile->CopySettings());
}
