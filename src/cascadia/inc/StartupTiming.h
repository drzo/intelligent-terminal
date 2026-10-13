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
    // Buffer checkpoints and emit an owned snapshot on a background thread.
    class StartupTiming
    {
    public:
        StartupTiming(const char* operation, const winrt::guid& paneId) noexcept :
            _operation(operation), _paneId(paneId)
        {
            Mark("begin");
        }

        StartupTiming(const char* operation, std::string context, bool enabled = true) noexcept :
            _operation(operation), _context(std::move(context)), _enabled(enabled), _hasPaneId(false)
        {
            Mark("begin");
        }

        StartupTiming(const StartupTiming&) = delete;
        StartupTiming& operator=(const StartupTiming&) = delete;

        void Enable(std::string context) noexcept
        {
            _context = std::move(context);
            _enabled = true;
        }

        void AppendContext(std::string_view context)
        {
            _context.append(" ").append(context);
        }

        void Mark(const char* stage) noexcept
        {
            if (_enabled && _count < _checkpoints.size())
            {
                _checkpoints[_count++] = { stage, std::chrono::steady_clock::now(), GetCurrentThreadId() };
            }
        }

        ~StartupTiming() noexcept
        {
            try
            {
                if (_enabled)
                {
                    Mark("end");
                    _Write(_operation, _paneId, _hasPaneId, std::move(_context), _started, _checkpoints, _count);
                }
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
        static winrt::fire_and_forget _Write(
            const char* operation,
            winrt::guid paneId,
            bool hasPaneId,
            std::string context,
            std::chrono::steady_clock::time_point started,
            std::array<Checkpoint, 24> checkpoints,
            size_t count)
        {
            try
            {
                co_await winrt::resume_background();
                auto message = fmt::format("startup_timing operation={} pid={}", operation, GetCurrentProcessId());
                if (hasPaneId)
                {
                    message += fmt::format(" pane_id={}", winrt::to_string(winrt::to_hstring(paneId)));
                }
                message += " " + context;
                auto previous = started;
                for (size_t i = 0; i < count; ++i)
                {
                    const auto& checkpoint = checkpoints[i];
                    message += fmt::format(" [stage={} elapsed_ms={} stage_ms={} tid={}]",
                                           checkpoint.stage,
                                           std::chrono::duration_cast<std::chrono::milliseconds>(checkpoint.at - started).count(),
                                           std::chrono::duration_cast<std::chrono::milliseconds>(checkpoint.at - previous).count(),
                                           checkpoint.threadId);
                    previous = checkpoint.at;
                }
                _agentPaneLog(message);
            }
            CATCH_LOG();
        }

        const char* _operation;
        winrt::guid _paneId{};
        std::string _context;
        bool _enabled{ true };
        bool _hasPaneId{ true };
        std::chrono::steady_clock::time_point _started{ std::chrono::steady_clock::now() };
        std::array<Checkpoint, 24> _checkpoints{};
        size_t _count{};
    };
}
