// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#pragma once

#include "AgentPaneLog.h"
#include <array>
#include <fmt/format.h>
#include <winrt/base.h>
#include <wil/result.h>

namespace IntelligentTerminal
{
    // Buffer checkpoints so diagnostic file I/O is outside the measured stages.
    class StartupTiming
    {
    public:
        StartupTiming(const char* operation, const winrt::guid& paneId) noexcept :
            _operation(operation), _paneId(paneId)
        {
            Mark("begin");
        }

        void Mark(const char* stage) noexcept
        {
            if (_count < _checkpoints.size())
            {
                _checkpoints[_count++] = { stage, std::chrono::steady_clock::now(), GetCurrentThreadId() };
            }
        }

        ~StartupTiming() noexcept
        {
            try
            {
                Mark("end");
                std::string message = fmt::format("startup_timing operation={} pane_id={} pid={}",
                                                  _operation,
                                                  winrt::to_string(winrt::to_hstring(_paneId)),
                                                  GetCurrentProcessId());
                auto previous = _started;
                for (size_t i = 0; i < _count; ++i)
                {
                    const auto& checkpoint = _checkpoints[i];
                    message += fmt::format(" [stage={} elapsed_ms={} stage_ms={} tid={}]",
                                           checkpoint.stage,
                                           std::chrono::duration_cast<std::chrono::milliseconds>(checkpoint.at - _started).count(),
                                           std::chrono::duration_cast<std::chrono::milliseconds>(checkpoint.at - previous).count(),
                                           checkpoint.threadId);
                    previous = checkpoint.at;
                }
                _agentPaneLog(message);
            }
            CATCH_LOG();
        }

    private:
        struct Checkpoint
        {
            const char* stage{};
            std::chrono::steady_clock::time_point at{};
            DWORD threadId{};
        };
        const char* _operation;
        winrt::guid _paneId;
        std::chrono::steady_clock::time_point _started{ std::chrono::steady_clock::now() };
        std::array<Checkpoint, 24> _checkpoints{};
        size_t _count{};
    };
}
