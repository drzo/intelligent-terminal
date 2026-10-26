// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#pragma once

#include <atomic>
#include <chrono>
#include <utility>
#include <TraceLoggingProvider.h>
#include <til.h>
#include <winrt/base.h>

TRACELOGGING_DECLARE_PROVIDER(g_hTerminalAppProvider);

namespace winrt::TerminalApp::implementation
{
    // Local ETW diagnostics only: no synchronous file I/O or telemetry keyword.
    class TabPerformanceTrace
    {
    public:
        using Clock = std::chrono::steady_clock;

        static bool Enabled() noexcept
        {
            return TraceLoggingProviderEnabled(g_hTerminalAppProvider, WINEVENT_LEVEL_VERBOSE, TIL_KEYWORD_TRACE);
        }

        static Clock::time_point Now() noexcept
        {
            return Enabled() ? Clock::now() : Clock::time_point{};
        }

        TabPerformanceTrace(const char* operation,
                            const uint64_t windowId,
                            winrt::hstring tabId = {},
                            const winrt::guid sessionId = {},
                            const Clock::time_point queuedAt = {}) noexcept :
            _enabled{ Enabled() },
            _operation{ operation },
            _windowId{ windowId },
            _tabId{ std::move(tabId) },
            _sessionId{ sessionId }
        {
            if (_enabled)
            {
                _id = _nextId.fetch_add(1, std::memory_order_relaxed);
                _parentId = _currentId;
                _currentId = _id;
                _entered = Clock::now();
                _started = queuedAt == Clock::time_point{} ? _entered : queuedAt;
                _previous = _started;
                Mark("Begin");
            }
        }

        TabPerformanceTrace(const TabPerformanceTrace&) = delete;
        TabPerformanceTrace& operator=(const TabPerformanceTrace&) = delete;

        ~TabPerformanceTrace() noexcept
        {
            Mark("End");
            if (_enabled)
            {
                _currentId = _parentId;
            }
        }

        void SessionId(const winrt::guid value) noexcept
        {
            _sessionId = value;
        }

        void Mark(const char* stage) noexcept
        {
            if (!_enabled)
            {
                return;
            }

            const auto now = Clock::now();
            TraceLoggingWrite(
                g_hTerminalAppProvider,
                "TabPerformance",
                TraceLoggingLevel(WINEVENT_LEVEL_VERBOSE),
                TraceLoggingKeyword(TIL_KEYWORD_TRACE),
                TraceLoggingUInt64(_id, "OperationId"),
                TraceLoggingUInt64(_parentId, "ParentOperationId"),
                TraceLoggingString(_operation, "Operation"),
                TraceLoggingString(stage, "Stage"),
                TraceLoggingUInt64(_windowId, "WindowId"),
                TraceLoggingWideString(_tabId.c_str(), "TabId"),
                TraceLoggingGuid(_sessionId, "SessionId"),
                TraceLoggingInt64(std::chrono::duration_cast<std::chrono::microseconds>(now - _started).count(), "ElapsedUs"),
                TraceLoggingInt64(std::chrono::duration_cast<std::chrono::microseconds>(now - _entered).count(), "ScopeElapsedUs"),
                TraceLoggingInt64(std::chrono::duration_cast<std::chrono::microseconds>(now - _previous).count(), "StageUs"));
            _previous = now;
        }

        void Count(const char* counter, const uint64_t value) const noexcept
        {
            if (!_enabled)
            {
                return;
            }

            TraceLoggingWrite(
                g_hTerminalAppProvider,
                "TabPerformanceCounter",
                TraceLoggingLevel(WINEVENT_LEVEL_VERBOSE),
                TraceLoggingKeyword(TIL_KEYWORD_TRACE),
                TraceLoggingUInt64(_id, "OperationId"),
                TraceLoggingUInt64(_parentId, "ParentOperationId"),
                TraceLoggingString(_operation, "Operation"),
                TraceLoggingUInt64(_windowId, "WindowId"),
                TraceLoggingWideString(_tabId.c_str(), "TabId"),
                TraceLoggingGuid(_sessionId, "SessionId"),
                TraceLoggingString(counter, "Counter"),
                TraceLoggingUInt64(value, "Value"));
        }

    private:
        inline static std::atomic<uint64_t> _nextId{ 1 };
        // Synchronous scopes only: never retain one across a coroutine suspension.
        inline static thread_local uint64_t _currentId{};
        bool _enabled;
        const char* _operation;
        uint64_t _windowId;
        uint64_t _id{};
        uint64_t _parentId{};
        winrt::hstring _tabId;
        winrt::guid _sessionId;
        Clock::time_point _started{};
        Clock::time_point _entered{};
        Clock::time_point _previous{};
    };
}
