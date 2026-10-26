# New-tab performance traces

New-tab diagnostics use the existing Terminal ETW providers at Verbose level with the shared
`TIL_KEYWORD_TRACE` diagnostic keyword (`0x100000000`), not a telemetry keyword.
They do not write a diagnostic file on the UI thread.
Timing and deferred initialization subscriptions are enabled only when the
corresponding provider is being traced.

| Provider | GUID | Events |
| --- | --- | --- |
| `Microsoft.Windows.Terminal.App` | `24a1622f-7da7-5c77-3303-d850bd1ab2ed` | `TabPerformance`, `TabPerformanceCounter`, `TabCreationContext`, `TerminalControlInitialized` |
| `Microsoft.Windows.Terminal.Control` | `28c82e50-57af-5a86-c25b-e39cd990032b` | `TerminalInitialization` |
| `Microsoft.Windows.Terminal.Connection` | `e912fe7b-eeb6-52a5-c628-abe388e5f792` | `ConptyStartup` |

## Collect

From an elevated PowerShell in the checkout being tested:

```powershell
New-Item -ItemType Directory -Force '.\Generated Files\TabPerf' | Out-Null
@(
    '{24a1622f-7da7-5c77-3303-d850bd1ab2ed} 0x100000000 5'
    '{28c82e50-57af-5a86-c25b-e39cd990032b} 0x100000000 5'
    '{e912fe7b-eeb6-52a5-c628-abe388e5f792} 0x100000000 5'
) | Set-Content '.\Generated Files\TabPerf\providers.txt' -Encoding ascii
logman start IntelligentTerminalTabPerf -ets `
    -pf '.\Generated Files\TabPerf\providers.txt' `
    -o '.\Generated Files\TabPerf\new-tab.etl'
```

Check that `logman` succeeds before reproducing. In the test app, create a tab,
wait for it to settle, then repeat at least three times. Keep cold first-tab
and warm subsequent-tab samples separate. Record the revision, configuration,
package identity, tab count, layout, profile opacity and agent pane position.
Start collection before creating tabs, not after a slow tab has opened.

Stop only this collection:

```powershell
logman stop IntelligentTerminalTabPerf -ets
```

Open the ETL in Windows Performance Analyzer and filter Generic Events to
`TabCreationContext`, `TabPerformance`, `TabPerformanceCounter`,
`TerminalControlInitialized`, `TerminalInitialization` and `ConptyStartup`.
The providers are shared by
Dev, Store and private worktree builds: filter to the tested process ID.
Do not stop another collection or replace another worktree's installed app.

## Interpret

`TabPerformance` contains `OperationId`, `ParentOperationId`, `Operation`, `Stage`,
`WindowId`, `TabId`, `SessionId`, `ElapsedUs`, `ScopeElapsedUs` and `StageUs`.
`OperationId` is process-local;
group by process ID plus operation ID. `ElapsedUs` is time since the operation
began; `StageUs` is time since its preceding marker. Nested operations overlap:
do not sum their totals. `End` is scope exit, including early returns or
exception unwinding; it does not imply success.

`ParentOperationId` identifies the enclosing synchronous App-provider scope
on the same thread, including reentrant notifications. Zero means no parent
was traced. `ScopeElapsedUs` excludes dispatcher queue wait. For exclusive
instrumented time, subtract only direct children's completed scope durations
from their parent's completed scope duration; do not subtract all descendants
or asynchronous lifetime events. This still includes uninstrumented callees
and trace overhead. Partial scopes at capture boundaries cannot establish
exclusive time or complete invocation counts.

Count complete `Begin`/`End` pairs under one new-tab operation to measure
notification amplification. `TabPerformanceCounter` records collection sizes
and projection choices for its owning operation, not an additional duration.
Tab/TabStrip-only scopes use `WindowId=0`; inherit window identity through
their parent where available. Scopes must never span coroutine suspension.

| Operation | Costs separated by its stages |
| --- | --- |
| `OpenNewTab` | Pane creation versus tab creation/selection (action/command-line entry point; COM tab creation skips this wrapper). |
| `MakeTerminalPane` | Settings/elevation, connection creation, control creation and pane wrapping; also covers dropdown entry points. |
| `CreateTabFromPane` | Shell-integration scheduling, tab object construction and initialization; covers all tab creation entry points. |
| `CreateControl` | Core creation, XAML construction and event/provider registration. |
| `InitializeTab` | Tab setup, header/collection insertion, foreground selection or hidden background attachment, and prewarm scheduling. |
| `SelectTab` | Unfocusing existing tabs, attaching content, focus/MRU work, theme/background, sidebar refresh and agent notification. Also emitted for ordinary tab switching. |
| `DeferredTabInitialization` | `Begin.ElapsedUs` includes queue wait; `ScopeElapsedUs` starts when the callback executes. Later stages separate wiring and prewarm handling. Startup/transfer deferral is included in the queue wait. |
| `PrewarmAgentPane` / `CreateVisibleAgentPane` | Agent resolution, master acquisition, helper creation, root split, synchronous helper layout, stash and bottom-bar refresh. |
| `ThemeColors` / `RecalculateTabColor` | Pane resources, titlebar, per-tab color application versus synchronous color-change consumers, frame and agent-bar updates. |
| `TabItemsVectorChanged` / `TabItemsChanged` | Sidebar display construction, insertion callbacks, viewport sync, visibility and page projection. |
| `TabListProjection` | Criteria, pane metadata, owner snapshots, shortcut hints, owner state application, represented sessions and history projection. |
| `SidebarHistoryCurrentSession` / `HistoryKeepRunning` | Visibility-gated history updates, snapshot/row scans and current-item application; `SkippedHidden` identifies calls that do no history work. |
| `WrapAgentPane` / `SplitPaneAtRoot` / `StashAgentPane` / `UpdateActivePane` / `ActivePaneChanged` | Helper XAML, settings, root insertion, hide/reparent, active-pane changes and their nested theme/projection notifications. |

`TerminalInitialization` separates core/interactivity initialization, connection
start, painting enable, appearance, scrollbar/automation and Initialized-event
consumers. `ConptyStartup` separates pipe/pseudoconsole creation, child launch,
pseudoconsole release, output-thread creation and connection-state consumers.
These separate DLL providers do not share App operation IDs. Correlate by
process/thread, session ID and enclosed event timestamps. A zero-size
initialization attempt can end without initializing; `RestoreQueued` does not
mean the restored connection has started. Neither event measures first Present
or shell-prompt readiness.

Join operations by nonempty `SessionId` or `TabId` within a process/window.
An early `Begin` can lack identity until it is assigned at a later stage.
Helper creation has its own session ID; its nested timestamps identify it
within the parent prewarm operation.
`TabCreationContext` records the existing tab count, layout, background-open
flag and startup replay flag, so growth with tab count can be distinguished
from cold-start or background-tab behavior.

`TerminalControlInitialized.ElapsedUs` measures from just before control/core
creation through first nonzero layout, connection start and renderer enable.
It is **not** first frame presentation, shell-prompt readiness or ACP readiness.
Reattached controls do not emit this creation-specific event. Use a graphics
Present trace or frame recording to verify that the transparent flash is gone;
an improved synchronous creation time alone does not establish that.

The performance worktree retains the baseline split entrance animation,
including hidden prewarming. The separate transparency fix is not included.
No opaque fallback background is added.

## Current-history row updates

`SetCurrentHistoryItem` now applies row UI changes only when the current-item
flag or resolved selection brush color/opacity changes. Equivalent brush
instances do not trigger another palette notification or container update.
When no history item is current, no selection palette is constructed.

This does not cache session identity or suppress history state updates:
keep-running status, snapshot projection and container realization retain
their existing update paths. Selection changes, resolved theme/contrast
changes and newly realized rows still receive their applicable state.
Snapshot commits that reuse a current row also clear that row's explicit
foreground and automation state when resetting its current-item flag.
Helper prewarming, layout, focus and session lifetime are unchanged.
This correction alone does not establish an end-to-end speedup; compare
matched captures before claiming one.

## Instrumentation smoke validation

A 2026-10-26 Windows x64 Debug capture of the independent
`tabperf20261026` package exercised three COM new-tab requests, starting with
one tab and leaving all three new tabs present until collection stopped.
It verified 637 complete App scopes, 539 parent-linked scopes, counters, and
Control/ConPTY startup stages for the new shells and hidden helpers.
ETW reported zero lost events or buffers. One `TabListProjection` scope was
truncated at the capture boundary and excluded from complete-scope counts.

This verifies emitted anchors and correlation, not first Present, an upstream
comparison or a performance improvement. The historical 16-tab measurements
below are a different scenario. Local smoke evidence, including
`anchor-validation.json`, is under
`Generated Files\TabPerf\AnchorSmoke-20261026-124917`.

## Matched current-history optimization measurement

The 2026-10-26 follow-up compares the instrumentation-only
`tabperf20261026` baseline with the independent `tabperfopt20261026` package.
Both use Windows x64 Debug, vertical tabs, 100% opacity, acrylic disabled,
a bottom helper and three warm COM requests starting with 1/2/3 existing tabs.
Each measured operation has 114 snapshot and visible history rows.
The revision is `acc38beb5eead22d5dc0a01e93ad2dd89957e478` plus instrumentation;
the optimized package additionally contains the current-history correction
described above. Neither package contains the transparency fix.

| Phase (ms) | Baseline samples | Optimized samples | Median change |
| --- | --- | --- | --- |
| Current-item application, cumulative within helper prewarm | 489.964 / 468.613 / 574.090 | 6.378 / 6.158 / 13.390 | 489.964 -> 6.378, 98.7% lower |
| Helper prewarm UI-thread scope | 1013.087 / 891.228 / 1052.293 | 595.511 / 394.309 / 781.830 | 1013.087 -> 595.511, 41.2% lower |
| Tab construction, registration and selection | 347.714 / 285.432 / 405.167 | 243.490 / 310.694 / 235.680 | 347.714 -> 243.490, 30.0% lower |
| Projection exclusive instrumented time within helper prewarm | 152.631 / 134.217 / 170.655 | 188.020 / 138.365 / 270.679 | 152.631 -> 188.020, 23.2% higher |

These are three samples per variant, not interleaved captures or a statistical
end-to-end speedup estimate. Inclusive rows overlap and must not be summed.
The projection code is unchanged; its variable timings do not establish a
new regression. Helper prewarm still blocks the UI for 394-782 ms in this run.
First Present, shell readiness, Release and upstream performance remain unmeasured.

### Findings

| Severity | Rule / location | Finding and impact | Disposition | Commit | Validation |
| --- | --- | --- | --- | --- | --- |
| HIGH | `PERF-UI-BLOCKING`, `PERF-MVVM-AMPLIFICATION`; `TabStrip::SetCurrentHistoryItem` | Repeated unchanged row writes consumed 469-574 ms per helper prewarm. | Local correction validated for the measured scenario; skips unchanged UI/automation writes without suppressing callers. | [58aaaef1b](https://github.com/microsoft/intelligent-terminal/commit/58aaaef1bebfc3505b47a169de68f5d02fbf5858) | Matched ETW: cumulative stage reduced to 6-13 ms; native behavior tests blocked before execution. |
| MEDIUM | `PERF-REPEATED-WORK`; `TerminalPage::_ApplyTabListProjection` | Seven projections remain per helper prewarm, with 138-271 ms exclusive instrumented time in the optimized capture. | Advice only; projection/theme batching is outside this correction. | None | Call counts and direct-child accounting verified; no projection optimization or saving claimed. |

### Checks performed

| Check | Scope / command | Result | Missing evidence |
| --- | --- | --- | --- |
| Native build | `Build-FocusedTests.ps1`; AppLib, App DLL, local tests and dependency-enabled TestHost builds | Completed; fresh App/test DLL hashes matched TestHost copies | Test bodies did not run |
| Focused behavior tests | `TE.exe TerminalApp.LocalTests.dll` with the four current-history color/pane/status/keep-running selectors recorded in `focused-tests.result.json` | Blocked: `Passed=0`, `Failed=0`, `Blocked=4`; local RPC authentication `0x8007000E` | Native behavior coverage remains blocked |
| Isolated package | `Build-PrivatePackage.ps1 -Token tabperfopt20261026`; `Deploy-PrivatePackage.ps1`, both with the separate optimization deployment directory | Completed; private payload hashes, restored source/ordinary host identity and unchanged baseline layout verified | Not ordinary Dev/Store integration coverage |
| Runtime trace | `Verify-AnchorSmoke.ps1`, `Verify-AnchorEvents.py`, `Analyze-Comparison.py` | Completed; 640 complete App scopes, 541 parent links, six shell/helper startups and zero lost events/buffers | No first-Present or CPU-sampling measurement |

The final capture retained the same five theme refreshes, seven projections,
and 15/20/25 current-history refresh calls per helper as the baseline.
The correction removes redundant row writes, not notifications, helper creation
or session lifetime. The initial follow-up capture used 89 history rows during
the measured operations and was excluded from the matched comparison; the
repeat waited for full history loading and verified 114 rows in every sample.

Local evidence is under `Generated Files\TabPerf\HistoryRefreshOptimization`,
including pinned source hashes, exact build/test command records and
`runtime-comparison.json`. The matched optimized capture is
`Generated Files\TabPerf\AnchorSmoke-20261026-153512`;
the excluded initial capture is `AnchorSmoke-20261026-153254`.
Both captures are preserved. Only their diagnostic tabs were closed.
Baseline and ordinary package registrations/settings were not replaced.

## Historical 16-tab baseline and investigation handoff

The 2026-10-26 capture used a Windows x64 Debug Dev build with vertical tabs,
100% opacity, acrylic disabled and a hidden bottom agent pane. Three warm COM
new-tab requests each started with 16 existing tabs. All measured phases ran
on UI thread 10196 in process 30492; ETW reported zero lost events.

The measured source was revision
`acc38beb5eead22d5dc0a01e93ad2dd89957e478` plus the transparency fix and tracing
changes, not that revision alone. The original analysis records tracked-diff
hash `18f7727558f4b212992a27a8943632adb2444f3042add4c38d3b528a4462c028`.
The deployed package was
`IntelligentTerminal.Worktree.tabflash20261025`, version `0.8.0.18`.

| Phase (ms) | Sample 1 | Sample 2 | Sample 3 |
| --- | ---: | ---: | ---: |
| Terminal object construction | 12.622 | 24.337 | 19.355 |
| Tab construction, registration and selection | 2028.770 | 1306.189 | 1187.877 |
| Tab collection insertion and callbacks | 390.900 | 388.103 | 410.810 |
| Selection BringIntoView, title and theme stage | 727.824 | 624.721 | 483.508 |
| Hidden helper prewarm | 2341.167 | 2312.675 | 2200.604 |
| Helper wrapping, wiring and root insertion | 1163.425 | 1132.992 | 1107.582 |
| Forced helper layout and initialization | 455.575 | 449.092 | 412.637 |
| Helper stash and synchronous notifications | 703.774 | 705.052 | 659.827 |
| Master acquisition stage | 0.145 | 0.152 | 0.241 |

Rows include overlapping parent/child scopes and must not be summed.
Low-priority dispatch postpones helper prewarming; it does not move its
synchronous work off the UI thread. These samples demonstrate substantial
responsiveness blocking, not that all elapsed wall time is CPU execution.

At the time of this earlier capture, the next attribution targets were repeated global theme updates and
sidebar/projection notifications during tab selection, helper insertion and
stash. Source inspection found that `_updateThemeColors` traverses all tabs
and their pane trees, including stashed panes; `Tab::ThemeColor` raises color
notifications, and root insertion and stash can each trigger active-pane
refreshes. Exclusive costs and notification counts were not yet measured.
`ThemeUpdated` is not exclusively `_updateThemeColors`, and `RootSplit` is not
exclusively the split function.

That investigation requested finer stages and refresh counts before choosing
an optimization, while preserving prewarming, Autofix availability,
helper/session lifetime and XAML thread affinity. The matched three-tab
comparison above records the subsequent attribution and localized correction.

This is not a keyboard-to-first-frame or shell-ready measurement. There is no
Release/upstream comparison, CPU-sampling profile or graphics Present trace;
the samples do not quantify regression against upstream.

The independent performance branch starts at main commit
`acc38beb5eead22d5dc0a01e93ad2dd89957e478`, without the transparency fix or its
tests. The migrated changes were initially diagnostic instrumentation only;
the current-history correction was added afterward.
The historical measurements above included the transparency fix, so this
worktree needs a matched 16-tab measurement before making same-baseline
performance claims.
Local evidence is preserved under
`Generated Files\TabPerf\Investigation20261026\AutomaticSamples`, including
`new-tab.etl`, decoded events, `analysis.json` and sample identities.
`Generated Files\TabPerf\Migration` records the transferred patch and source
hashes. These artifacts are ignored by Git; historical capture scripts and
paths describe the original run and must not be treated as a new deployment.
