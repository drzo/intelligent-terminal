# Antigravity ACP initialization failure storm: investigation and implementation

## Feature Metadata

- Date: 2026-10-19.
- Summary: investigate automatic Antigravity ACP preparation followed by repeated
  initialization failures, native error dialogs, and accumulating processes.
- User-visible goal: prevent every provider's failed ACP startup from causing
  unbounded retries or retained descendants, using common lifecycle policy and
  provider-specific preparation adapters.
- Base: `origin/main`, fetched on 2026-10-19;
  `a73ca6b8aa76acec1568465a445f59e8409725f8`.
- Base commit timestamp: 2026-10-19 10:34:31 +08:00.
- Dev branch: `dev/xiaomgao/antigravity-acp-init-storm-20261019`.
- Dev worktree:
  `C:\ado\intelligent-terminal-antigravity-acp-init-storm-20261019-dev`.
- Publish branch:
  `dev/xiaomgao/antigravity-acp-init-storm-20261019-publish`.
- Publish worktree:
  `C:\ado\intelligent-terminal-antigravity-acp-init-storm-20261019-publish`.
- Dev/publish remote: `origin` (`microsoft/intelligent-terminal`). At 12:08:35
  +08:00 on 2026-10-19, the user approved implementation through a merge-ready
  PR. Push dev first, then the independently validated publish head. Do not
  merge the PR.
- Related product PR: `microsoft/intelligent-terminal#1155`, merged as
  `ccdf84589cfc4d2d63af750c4050ea94565f767f` on 2026-10-16.
- External issue: `google-antigravity/antigravity-cli#921`.
- Evidence root: `artifacts\antigravity-acp-init-storm-20261019` in the dev
  worktree. This directory is ignored.
- Refactor product commits on dev:
  `b8253d73af5f6a79d3ed1b688400de61884fb89b`,
  `1920ba30909b78480fc1b2a1e9a4c2ffb0cd786a`,
  `2ef93e5bc800541858d8b3defc97b380d52fe44b` and
  `f4a027d3c35e3c767abdb94939c59150e317a024` and `72fb4f337`.
- Direct cherry-picks on publish:
  `7301970aa3c40dfd07e1f3a71f541ab19d5b7912`,
  `1b0961026720dedb9debe3293141abf2f0993197`,
  `ba61c2929c3494d380fe2aca30ce83989884afe3` and
  `7803dd855d15c52c46819288f28f8f2077779019` and
  `bde1fc453bb5e98d0f3b9555d7bef3362dae2539`.
- Current local refactor publish head: `86f88c1a537fe613580690562a66bea275e8c205`.
- Refactor PR: `microsoft/intelligent-terminal#1180`.
- Current implementation base: `d2fdbd75fed3d881b069c642a6c200734dbe09a7`.
  A final fetch confirmed that this is still `origin/main` and an ancestor of
  the publish head.
- Original dirty worktree: `C:\ado\intelligent-terminal`, left untouched.

## Current Stage

### Critical check assessment and current-main alignment

Settings follow-up review `5586234276` at `2026-10-20T14:09:06Z` genuinely
converged on `1f4e4d79e` with no concrete findings. The user's requested critical
check analysis then inspected real completed artifacts/logs instead of treating
red statuses as either product bugs or automatic false positives.

- Security failed before review: recorded PR base `d2fdbd75` differed from live
  main `3b0ff06d`. Its immutable-baseline guard was retained.
- Native source review claimed no findings but failed `review_checks must cite
  every classified changed file`. Its separate three-state Windows Axe smoke
  passed. This is an incomplete report, not complete accessibility approval.
- Performance worker `39413632868` exceeded its 15-minute execution limit and
  produced no Markdown summary; detector follow-up also lacked `threat-detect`.
  The additional report check is a second display of the same failed worker.
- Main already contains reviewed workflow startup/summary fixes. After a clean
  preview, exact main `3b0ff06d` was merged normally into dev `c6e15f552` and
  publish `86f88c1a5`; no guard or trust condition was relaxed.

Integrated validation: exact publish WTA 2,998 unit +2 CLI tests pass, six opt-in
ignored; final Clippy and two explicitly selected WSL ownership controls pass.
Windows hook tests pass 33/33. Workflow tests pass 96 with three platform skips
on native Linux fixtures; accessibility contract tests pass 111 and localization
workflow tests pass eight. Initial local fixture failures were diagnosed as
missing VM-module options, Windows NUL hook-path handling and NTFS file modes;
the guards stayed enabled. One owned hung Linux hook test was identified by
exact executable/parent/arguments and stopped by PID; Windows hook coverage
passed separately.

An additional Rich Tab literal-path fixture fails identically on unmodified
main code: the escaped `git[1].cmd` application path is not resolved by the
selected PowerShell. The diagnostic assertion was not weakened and temporary
logging was removed. This unrelated baseline failure is recorded, not counted
as a passing full native suite; focused ACP parsing passes 20/20.

Full private package **0.8.0.23** (`ci-aligned-publish-013`) is deployed from
`86f88c1a5`, including the new static OpenCode hook assets. All four binaries
match MSIX `3EAE176C477CDDB83524D538DF9DCB37E407D39DB701C19BAEB41228186A9F9F`;
WTA is `4FEEBE16A49B484B02C69E4FEA8E7E25F73F688EED7C4E7381F616D23F39AB5F`.
The four lifecycle E2E cases, including native text selection/copy/paste, pass;
C455-C458 remain uniquely mapped. User settings/state/runtime and clipboard
are preserved. Source identity overrides are restored and normal outputs rebuilt.
The short user-facing check explanation is `local-investigation\PR-1180-CHECKS.md`
on DEV only. New-head publication/review/check observation is still pending.

### Factual Settings errors and selectable native text

The user rejected generic retry advice in Settings failure messages and asked
for a factual failure plus native details that can be selected/copied. Dev
`a060bfa34` / publish `1f4e4d79e` remove retry/network-remediation prose from
installation/startup summaries across all 18 locales. The selected agent and
explicit retry button remain available; no retry limit or cleanup logic changed.
Native details now appear immediately in an enabled, read-only wrapping TextBox,
not a collapsed Expander.

The deployed 0.8.0.21 baseline failed the new exact factual-summary assertion
(RED). The final 0.8.0.22 package at `factual-errors-publish-012` passes all four
ItE2E lifecycle cases. The focused case proves read-only UIA text, partial
selection, real guarded Ctrl+C, and Ctrl+V into a separate editable Settings
search field. The prior clipboard data object is restored in `finally`, and
current user settings/state/runtime files are preserved. Physical mouse drag
automation is explicitly blocked by the existing harness and was not bypassed;
the product uses standard TextBox mouse-selection behavior, not a custom input
implementation.

All 90 scoped localization checks pass after independent language review.
The zh-TW installation summary now uses the established modern 智能體 term.
Arabic keeps the existing Settings term; its pre-existing cross-component WTA
variation was noted, not silently expanded into unrelated translation changes.
Original encoding and bytes outside the three changed entries are preserved.

Both normal and private native builds pass with baseline PRI warnings. Rust was
not rebuilt: its source tree and explicit-target executable remain identical to
the previously validated `a78568f42` implementation. All four installed binaries
match the 0.8.0.22 MSIX:
`6BED821B1EF39A3823548860A895C2B068CD810487904B92CA29F980DCB75455`.
Evidence is under `Generated Files\acp-settings-error-copy`.

Publication/review of this follow-up remains. The user also explicitly requested
critical check-result analysis after comments are handled, with a short,
plain-language DEV-only explanation of experimental check failures or
inapplicable assumptions. That is tracked separately; no failing assessment
will be declared safe merely because it is optional or experimental.

### Completed native-details implementation and review convergence

All requested code, publication and deployment work is complete. Private
**0.8.0.21** remains registered at `native-details-publish-011`; its final WTA
is SHA-256 `9522F430C23AFDBC8D521E1FBFEEDA8C68C4606920CDEDF7845082E246FA8AA1`
from publish `a78568f42`, with the full `1db0dc548` native carrier and original
MSIX preserved. Native error details are visible in the helper and the separate
localized Settings section. User settings/state and existing runtime files
remain byte-exact. Final GUI/enterprise acceptance, screenshots and correctly
mapped C455-C458 reports were inspected.

Copilot review **5584308163**, submitted **2026-10-20T11:21:25Z**, is at exact
HEAD `a78568f42571fd7deb43c5425d1740f88df7fdff` and has no new findings.
The parent independently reran the strict checker after final cleanup:
`Converged=true`, no pending review, zero open/unanswered threads. The full body
was inspected; its independent-human-review recommendation is not a code defect
or a fabricated approval.

Spelling, CLA and localization pass. The final security controller failure was inspected:
it refused to replace an immutable stale base before dispatch. That safeguard
was retained, and the failed assessment is not counted as clean. The optional
native source-review job failed and the performance worker failed without a
readable summary. The separate native Windows smoke remained in progress.
These are not passing assessments. The native job log was not available through
the run viewer while the smoke was active, and direct output was refused for
terminal escapes; that output safeguard was not disabled.
The PR is mergeable but awaits its required independent non-author approval.
No account was switched, approval forged, rule changed or PR merged.

### Canonical legacy/diagnostic identity and final spelling follow-up

Review `5583881808` at `2026-10-20T10:35:05Z` identified a real final identity
split: a legacy omitted built-in ID and an inferred explicit ID selected
different shared keys. Dev `6ed864ec8` / publish `a78568f42` canonicalize only
omitted known IDs in the shared key builder; explicit custom IDs remain distinct.
Three tests were RED before the correction and GREEN afterward, proving shared
exhaustion, quarantine and retry reset without launching real providers.

Six spelling annotations were handled narrowly: reword the UTF-8 prose, rename
the blocked-input test, use `char::REPLACEMENT_CHARACTER`, and add the real
Linux procfs `wchan` term to the existing expectation list. No check was disabled.
The security workflow's failure was separately inspected: it refused a stale
immutable base before dispatch. It is not a clean assessment or a product
vulnerability finding, and its guard was not bypassed.

Exact publish validation passes **2,995 units plus two CLI integrations**,
zero failures, six opt-in ignored cases; both WSL ownership tests were also
explicitly selected and pass. The private 0.8.0.21 native carrier is unchanged.
Its inactive WTA payload alone was refreshed to
`9522F430C23AFDBC8D521E1FBFEEDA8C68C4606920CDEDF7845082E246FA8AA1`,
with the previous binary and complete original MSIX retained.
`e2e-canonical-runtime` passes 4/4 and reports credit only C455-C458.
`enterprise-canonical` confirms one initialize, generic failure, original
Python DLL/LoadLibrary details, zero recorded survivors and preserved settings.
Publication, replies and final-head review remain pending.

### Final full package and independent WSL host-lifetime signal

The concurrent localization bot pushed `511991dd7` while publication was in
progress. The normal push safely rejected; no force push was attempted. Its
Arabic/Serbian startup-message corrections match existing RESW/YAML agent-pane
terminology and pass 12 focused checks. They were preserved in dev `3891ee851`
and publish merge `8fd3f187c`, which received genuine current-head Copilot
convergence at `2026-10-20T09:32:10Z`.

An intervening review body (`5582874030`) also described stdin-backpressure
blocking the WSL EOF guardian. This was not ignored just because a later summary
had no open issues. Dev `9dd1589ce` / publish `1db0dc548` fix it:

- A real owned relay was observed in `anon_pipe_write` with 1 MiB queued.
  The former guardian then failed the bounded host-exit oracle. Earlier attempts
  that assumed the kernel wait-channel name or newline format were fixture
  diagnostics, not accepted RED. The RED run explicitly reclaimed its group.
- An independent stderr-closure guardian now detects host exit while stdin
  forwarding is blocked. One blank line per second is filtered before logging
  and startup retention. Lossy stderr decoding preserves reader lifetime when
  native diagnostics are not UTF-8; ACP stdout is untouched.
- Exact publish tests pass 2,992 units plus two CLI integrations, with six
  opt-in ignored cases. Both relevant WSL tests were explicitly run and pass:
  blocked host exit, healthy echo after heartbeat intervals, idle host exit,
  and ordinary owned cleanup. Clippy passes with baseline warnings.

The complete final **0.8.0.21** private package is deployed at
`native-details-publish-011`; all four installed binaries match the exact MSIX
`35D0AECE0F90C8531046160824B99A8173BF1BEE44A7ADDB25BDE9655DDA997F`.
WTA SHA-256 is
`2A0E36BD8ED46377F4AD72B498A030F97BDC6E0E5FC8AA298B2CA48ACB739E16`.
All four packaged lifecycle cases pass, reports uniquely credit C455-C458,
and real Antigravity verification again exposes the Python DLL/LoadLibrary
failure with one initialize and no recorded survivors or model requests.
Current settings/state and existing runtime files are preserved. Private
source identity substitutions are restored byte-exact and ordinary outputs
rebuilt. Final-head publication/review remains; the older `8fd3f187c`
convergence is not proof for `1db0dc548`.

### Final diagnostic-admission review follow-up

Review `5582626179` at `2026-10-20T08:31:31Z` examined `f38ce353c`.
It exposed a genuine remaining diagnostic gap: `probe-sessions` and
`probe-host-sessions` still prepared/spawned outside shared admission.
Dev `691564a41` / publish `6c1c868f6` now route both through one shared
session-list startup path. It snapshots inherited configuration, reuses the
master's stable binding digest, retains the lease through cleanup, and persists
startup/cleanup failures. Explicit retry cannot override quarantine. The
common 60-second initialize deadline replaces the older diagnostic 10/25-second
split; listing stays at 10 seconds and unsupported-list outputs are preserved.

A real CLI integration test first observed four launches instead of three.
GREEN covers the shared ceiling across both commands, no starts after
exhaustion, explicit recovery, no starts after an isolated persisted quarantine
even with retry requested, and no auth/session/prompt requests.
Final exact-publish tests pass 2,990 units plus two CLI integrations with five
existing opt-in ignored cases. Clippy passes with unchanged baseline warnings.

The spelling finding was an escape-adjacent synthetic word; `concat!` preserves
the exact CR-test bytes without changing a spelling allowlist. The other
Copilot concern mistook a masked credential fixture for a literal asterisk
input. The exact regression passed 1/1 on unchanged `f38ce353c`; an explicit
harmless-asterisk control and clarified assertion message now document the
distinction. Sanitizer behavior was not changed to satisfy that rendering.

The inactive private 0.8.0.20 carrier was WTA-only refreshed again. Current WTA:
`5B35B8FEE4A6BD836793AEC3EE9CF04ACFC3D4124852A14A64387667CFE22C65`.
All native binaries, the original MSIX and prior WTA copies remain intact.
`e2e-diagnostic-runtime` passes 4/4; current reports uniquely credit C455-C458.
`enterprise-reviewed` verifies the exact new binary still returns the real
Python DLL / LoadLibrary reason with one initialize, generic failure, no
remaining recorded processes and no model requests. User configuration remains
preserved. Review replies/publication and new-head convergence are pending.

### Native error details and current-main integration

The user confirmed that safe native error details should be displayed. A
read-only correlation of their real 0.8.0.19 failure showed that master had
already captured the Python DLL path and the native LoadLibrary rejection;
`helper_initialize_error` had forwarded only a summary and structured codes.
This was a propagation/display gap, not absence of an underlying diagnostic.

Native-details implementation is dev `b4ef05a` / publish `9ef8902e0`:

- Optional `StartupFailure.details` carries at most four lines / 1,024 UTF-8
  bytes of conservatively filtered stderr, or structured RPC/OS message text.
  The existing authentication reason filter is reused and extended to common
  API/private-key labels; control/bidi characters and sensitive tails are not
  forwarded. Provider data objects stay private. Details do not infer causes.
- Master, probe/installation verification and history startup attach the same
  context. The helper renders it without changing the failure/recovery type.
  Settings keeps its localized primary InfoBar and exposes native data in a
  separate, initially collapsed Expander that clears on retry/selection changes.
- One localized header was added in all 18 locales. An independent read-only
  review accepted all translations; 90 file checks pass. Original mixed line
  endings outside the new entry are preserved, avoiding unrelated resource churn.
- Helper display and the real CLI first failed their new native-detail oracles,
  then passed. Source-level transport and actual helper render tests pass.
  Native parser/model tests pass 20/20; pre-main full Rust passes 2,915 plus
  one CLI test with five existing ignored cases, and Clippy passes with warnings.
  The existing localized preview assertion now holds the normal locale guard
  after an observed parallel-only failure; its assertion was not weakened.
- C450 now checks the separate localized details header, expansion, native text,
  and clearing after recovery; C451 checks native detail propagation while
  retaining the generic `failed` reason and cross-process ceiling.

The body-only runtime cleanup review finding is fixed locally in dev
`e0b2be846` / publish `38cc3a1bd`. A real receipt-waiting process made independent
admission incorrectly succeed before the fix. The corrected reaper waits for
exit/stop without reserving a healthy process's lease, then holds admission
through termination and cleanup-failure persistence. Pending-cleanup, healthy
process and quarantine controls pass; real Ubuntu-24.04 containment also passes.

Main advanced to `d2fdbd75` during this work. Both branches now integrate it
(dev `cb0c2c9a`, publish `6da09358a`), retaining both native-observation fields
and lifecycle state. Three textual conflicts were resolved by preserving both
features, and seven lifecycle tests use the new explicit partial-inventory
argument. Integrated dev Rust is green: 2,988 unit tests plus one CLI test,
zero failures and five existing ignored cases. Dev/publish WTA and native
source tree hashes match.

Final acceptance refinements are dev `a624fddbb` / publish `f38ce353c`:

- Real Windows stderr ended with an extra carriage return. The first deployed
  result retained both useful DLL lines but marked that harmless suffix redacted.
  Normalize trailing CR as a line ending; interior controls still terminate
  safe details. The new regression and final exact-publish full suite pass:
  2,989 unit tests plus one CLI test, zero failures, five existing opt-in ignored.
  Final Clippy passes with repository/main warnings.
- Expander realization is asynchronous. The first packaged details assertion
  queried its child too early. Waiting for the visible native Text element
  retains the exact text/name assertion; the full four-case rerun passes.
- Main independently assigned C450-C453 to native-status cases. Remove only this
  PR's colliding assignments and run Set-ChecklistIds: lifecycle cases are now
  **C455-C458**. Each new ID is unique; main's existing mappings/multiplicities
  remain unchanged. Full/incremental reports credit only these four lifecycle
  cases, not main's unexecuted C450-C453 cases.

The private package is now **0.8.0.20**, layout `native-details-publish-010`.
Its complete native/MSIX carrier is from `6da09358a`, with exact-binary acceptance.
Only WTA was then refreshed from `f38ce353c` after another inactivity check;
current WTA SHA-256 is
`16256F3F7ECA408F49A0759E7DC42A22CAA1984505687893663D525177852B3B`.
The previous 208B34AC executable and original MSIX are preserved, not represented
as containing the refreshed WTA. All three native binaries are unchanged.
The final-runtime packaged suite passes 4/4 with no skips. Current settings/state
were backed up afresh before upgrading, and only creation-owned test processes
were stopped. Source identity overrides were restored byte-exact and normal
build outputs refreshed.

Real Antigravity verification on the 0.8.0.20 carrier returned an installed but
failed single initialize, generic `failed`, native Python DLL / LoadLibrary
details, and zero remaining recorded launcher/PYI processes. The first evidence
script looked in `wta-probe` incorrectly; the already completed operation was
correlated in `wta-cli` without starting another ACP. The fixture's RPC code is
not represented as a Windows DLL status code. Final refreshed-runtime
verification also passed: one initialize, native DLL/LoadLibrary details without
false CR redaction, two recorded launcher/PYI PIDs both gone, and no model
requests. `enterprise-final\receipt.json` records the exact 16256F3F binary.
Publication and current-head review remain; no clean review is inferred from
the old review summary or from empty thread lists.

### Prior policy-category removal and 0.8.0.19 deployment

2026-10-20 UTC: the user rejected the special `operating_system_policy`
category inferred from English stderr and explicitly authorized its removal.
Product dev commit `72fb4f337` and direct publish cherry-pick `bde1fc453`
remove the enum variant, `diagnose_stderr`, both probe/master call sites and
the C++ parser's corresponding allowed value. No replacement message heuristic
or new raw-error UI was introduced.

The remaining reasons are `failed`, `timed_out`, `authentication_required` and
`retry_limit_reached`. Structured native OS/RPC codes, bounded startup stderr
with the existing authentication redaction, the three-attempt budget, explicit
retry epochs and owned process cleanup remain intact. Phase-based localized
Settings presentation and provider selectability are unchanged.

Evidence is under `Generated Files\acp-policy-removal`:

- Rust serialization and C++ parsing regressions both failed against the old
  accepted category, then passed after its removal.
- The old deployed WTA classified a deterministic fixture's harmless
  "Application Control policy has blocked" text as `operating_system_policy`,
  despite the fixture returning an ordinary structured RPC error. This CLI RED
  is preserved in `cli-red`, not rewritten as new-build evidence.
- Both dev and exact publish full WTA runs pass: 2,905 unit tests plus one CLI
  integration test, zero failures and five existing opt-in ignored tests.
  The earlier clipboard prerequisite failures did not recur in these runs;
  no clipboard permissions, test assertions or skip controls were changed.
- Native ACP parser/model tests pass 18/18 on both dev and exact publish.
  The full incremental ordinary Dev package build also passes, with compiler
  and PRI warnings. Source identity inputs remain byte-exact ordinary Dev;
  their timestamps were refreshed to avoid reusing earlier private-brand objects.
  This package was built only, not installed over any registered layout.
- Clippy passes with existing repository warnings; changed Rust matches rustfmt,
  changed C++ is range-formatted, and the modified PowerShell files parse.
- `cli-publish-green\receipt.json` ties the live CLI regression to clean publish
  `bde1fc453` and WTA SHA-256
  `15BAB6712C9A4EDD57941A8441388306770FE3FAAB88E8967E9B18606E758A51`.
  The former trigger phrase now yields `failed`; RPC `-32603` and diagnostic
  stderr are retained. Repeated probe processes stop at three starts, ordinary
  requests stay blocked, explicit retry recovers on start four, and owned
  fixture descendants are gone before return. Failed verification makes one
  attempt; successful verification initializes once without a session or prompt.
  This is isolated unpackaged CLI evidence, not packaged GUI E2E.
- C451 now asserts generic classification, preserved RPC code and diagnostic
  stderr in addition to its existing cross-process ceiling/retry contract.
  The prior four-case packaged pass predates these new assertions.

The first deployment check found the private 0.8.0.18 app active, so it was
preserved. The user then explicitly closed it and requested the latest build.
Publish `bde1fc453` is now deployed as private **0.8.0.19**, immutable layout
`policy-removal-publish-009`, with the same PFN, aliases and protocol CLSID.
No package was uninstalled and no user-owned process was terminated.
The old receipts' policy category remains historical, not the current contract.

The four packaged lifecycle cases now pass on this exact build, including the
strengthened C451 reason/RPC/stderr assertions. Both screenshots were inspected,
and full/incremental reports credit C450-C453. The larger report count is not a
claim that more than four cases ran. `settings.json` and `state.json` remain
byte-exact; both existing private runtime files were preserved after inactive
teardown. Only the test-replaced `master-pipe.txt` needed restoration from its
local backup. All four installed binaries match the exact private MSIX
`DE00DDAA6653E29939C0D61790FA78626FFA7130874A738117E2BE1C6A527397`.
Receipts and results are under `Generated Files\acp-policy-removal\private-package`.
The three temporary private source substitutions were restored byte-exact and
the normal package outputs were rebuilt; the registered private layout remains
separate. The user can reopen this build for manual testing.

Current-head Copilot review `5581009913` at `2026-10-20T05:13:31Z` is **not
converged**. Although its overview says "No open issues", its body contains a
previously missed MEDIUM at `master/mod.rs:6268`: the initialized-process reaper
calls `AgentProcess::wait()` (which terminates descendants) before acquiring the
shared startup lease. Independent callers can admit work during that cleanup.
The strict review detector correctly reports `NoNewComments=false`; absence of
a separate review thread is not resolution. Investigation confirmed the
ordering and found the existing `HistoryProcess::cleanup` lease pattern.
No fix for this new finding has been implemented yet. Keep the user's current
manual-test installation unchanged while working on a separate code correction.
Independent non-author approval also remains external.

### Prior autonomous review and approval handoff (7803dd855)

At this earlier stage, the user requested further autonomous work, not a
premature completion. New evidence and repairs are under
`Generated Files\acp-review-followup` (ignored):

- The current review-summary detector had a genuine false negative for the
  observed `ccr-overview-v2` "No open issues" field. Dev-only commit
  `7a0750727051dcf34c2d4cefb5f6e970a52f03ed` adds strict format compatibility,
  not a convergence override. Four new tests failed first; all 84 tests now
  pass, retaining stale-head, unanswered-thread, pending-review, contradiction
  and historical-section guards. Real reviewed-head convergence passed under
  PowerShell 7 and Windows PowerShell 5.1; no outdated threads remained.
- A read-only native probe confirmed `OpenClipboard` returns
  `ERROR_ACCESS_DENIED (5)`. It neither read nor changed clipboard contents and
  did not change desktop/permission policy. The two baseline setup failures
  remain genuine prerequisite failures.
- Native-report diagnosis found 22 declared source files but only four validly
  cited files, 18 missing citations and five invalid immutable line references.
  The validator correctly rejected the generated report. No report was edited
  into a fake CI pass, and no validator/tool permissions were loosened.
- A genuine independent local performance review inspected all 61 product files
  at `ba61c2929` and found a MEDIUM responsiveness regression: retry/post-login
  initialization waited for a history mutex merely to call an empty method.
  The parent implemented a normal correction of this introduced regression,
  without promoting severity or invoking a HIGH-only automatic repair.
- The corrected regression uses a real pending mock history RPC and a healthy
  pooled agent. It is RED before the fix and GREEN afterward while history is
  still pending; it also proves stable agent ownership, one pooled agent and
  persisted retry epoch 1. It uses paused virtual time, not a latency benchmark.
  Its explicit native binding and nonexistent fallback command prevent an
  unintended provider launch on fixture mismatch. An initial mismatched-binding
  fixture failure is preserved separately and is not counted as RED.
- Dev `f4a027d3...` and direct publish cherry-pick `7803dd855...` remove only the
  redundant gate/no-op and obsolete calls, retaining all epoch/cleanup assertions.
  The same performance reviewer independently verified the delta and recorded
  RED/GREEN, resolving that finding with no additional actionable delta finding.
  Slow-storage persistence and probe-retention measurements remain LOW advice.
- Required full WTA tests: 2,902 passed, the same two clipboard setup failures,
  five opt-in ignored. The exact publish-head unit run repeats that result.
  CLI integration, real Ubuntu-24.04 containment and Clippy also passed on the
  identical dev WTA source; changed Rust matches formatter output.
- Only WTA was rebuilt/refreshed for this Rust-only correction. No full package
  deployment was performed. The inactive 0.8.0.18 private carrier from
  `ba61c2929` retains its C++/XAML binaries; current WTA from `7803dd855` has
  SHA-256 `DE03B85B98E0F939A4C9B3BD94428C3BC581CA55C470C0FD3F82AED601415430`.
  Previous WTA and original MSIX are preserved. The installed directory is now
  an explicitly WTA-refreshed carrier, not the original immutable MSIX payload.
- New exact-runtime packaged E2E passed 4/4 with quoted paths; fresh failure and
  recovery screenshots were inspected, and both reports credit C450-C453.
  Real enterprise acceptance again observed one policy-blocked start, provider
  selectable, no native dialogs and no owned survivors.
- The validated product commit was pushed after dev, with a new-head Copilot
  review completed: `5579024902`, `2026-10-19T23:35:32Z`, matches `7803dd855...`,
  has no open/suppressed findings, and legitimately reports `Converged=true`.
  Prior-head review/check outcomes are not new-head proof.
  No permission, timeout, policy, report-coverage or test safeguard was bypassed.

Repository merge rules were inspected, correcting an earlier overstatement:
the failed automatic review workflows are not required status checks. GitHub's
mandatory remaining condition is at least one non-author approval. Current
`gh` identity and PR author are both `DinahK-2SO`; no alternate account was used,
no approval was fabricated, and no rule was changed. The user was asked to
provide the external approval handoff, but was unavailable and will review later.

Current-head optional review outcomes: localization/spelling and Native
Axe.Windows smoke pass; native source
report fails validation with no reported defects; performance times out with no
summary; security reports partial execution with only a scope stub, not a clean
assessment. Native smoke job `118369859665` passed at
`2026-10-20T00:04:16Z`; the overall native workflow remains failed because its
source-report publication check failed. All observed runs are now terminal.
The independent local performance assessment and actual runtime evidence are
documented as supplements, not forged passing GitHub checks.

All autonomous implementation, review triage, validation and publication work is
complete. The remaining mandatory non-author approval is an external handoff:
the user was unavailable and will review later. No unfounded reviewer selection,
account switch, approval, branch-policy override or automatic merge was made.
The task must not be described as already approved or merged.

### Prior accessibility-complete head

2026-10-19 UTC, latest stage: the accessibility follow-up is implemented,
validated and pushed as `ba61c2929...` after dev `2ef93e5bc...`. Recovery of the
unpublished native report exposed a real HIGH finding despite its failed report
contract, so work did not stop at an infrastructure-only handoff.
The ownership hypothesis was confirmed: appending phase/reason/count/GUID data
polluted the localized primary error, and custom InfoBar content did not use its
standard message/action semantics.

C450 was RED on the exact `1b096102`/0.8.0.16 package at the localized UIA-text
assertion. The fix retains technical diagnostics in logs/CLI, uses localized
InfoBar.Message plus ActionButton, and notifies the message before opening.
The candidate and exact publish package both passed all four E2E cases, including
quoted paths and the new UIA assertion. Exact-head native tests passed 17/17;
full Rust remains 2,901 passed, two baseline clipboard failures, five opt-in
ignored. The WTA subtree is unchanged from `1b096102`.

Exact package 0.8.0.18, immutable layout `a11y-publish-008`, passed fresh
enterprise acceptance with one start, OS-policy failure, selectable provider,
zero dialogs and zero survivors. Fresh screenshots were inspected. Source,
MSBuild staging, immutable MSIX and installed hashes match, including the
changed Settings Editor DLL. Both source worktrees' private identity inputs
were restored; publish is clean.

New-head Copilot review `5578063661`, submitted `2026-10-19T21:35:28Z` at
`ba61c2929c3494d380fe2aca30ce83989884afe3`, reports no open issues and no
suppressed findings. It is COMMENTED, not human approval. Current-head remote
checks have all finished: security, localization, spelling, CLA and Native
Axe.Windows smoke passed. The native source-review workflow still failed its
report publication-policy validation, and the performance worker timed out
without a summary. Their exact-run artifacts were inspected; native findings
are empty, while performance findings remain unknown because the assessment
did not complete.

Current stage is blocked handoff, not merge-ready. GitHub reports a mergeable
diff but `BLOCKED` / `REVIEW_REQUIRED`. No further demonstrated product defect
remains from the completed reviews. Review execution/report failures and human
approval need disposition outside this product patch. Manual Narrator/keyboard
timing and contrast/theme/scaling requests remain unclaimed. No test/review
safeguard was weakened, and no validation command remains running.

### Previous accepted head, before the accessibility follow-up

2026-10-19 UTC: implementation, both Copilot code-review rounds, exact-publish
package validation and real enterprise expected-failure acceptance are recorded.
That publish head was `1b0961026720dedb9debe3293141abf2f0993197`; PR #1180
is ready for review, but not merge-ready. Security and localization passed.
Native Axe.Windows smoke at this head passed at `2026-10-19T20:21:20Z`.
Performance review failed its original attempt and one genuine retry, both
after the worker exceeded its existing 15-minute execution limit and failed to
deliver a summary. The ready-event native accessibility agent failed generated
report coverage validation; exactly one native failed-jobs retry is running.
No further retries or workflow-limit changes are authorized by this handoff.
The local full Rust suite is not green because two clipboard setup failures
also reproduce on unchanged main. The review helper's old no-comments parser
does not recognize the new review format, so its `Converged=false` output is
preserved rather than overridden. Next: observe the already-started native
retry to a definitive result and document the blocked gates for the merge owner.
The gate monitor is read-only again: performance run `39313413985` attempt 2
failed, accessibility `39313414119` attempt 2 is running, security `39313414048`
and localization `39313413952` passed.

Product changes include the
provider adapter registry, shared persisted three-attempt admission, owned
Windows/WSL process cleanup, one-shot installation verification, structured safe
startup diagnostics, explicit helper/Settings retry, and localized Settings
errors that leave selection enabled. Review fixes add stable cross-restart
binding digests, persisted explicit-retry epochs, a separate overall helper
deadline, cancellation-safe history receipt ownership and post-initialize
cleanup quarantine.

Key evidence:

- `cross-probe-red`: two baseline CLI requests started six processes.
- `cross-probe-green`: independent callers started only three in total.
- `manual-retry-green`: ordinary requests remained stopped at three; explicit
  retry started exactly one recovered provider and returned its model.
- `install-verification-red`: baseline installation reported success without a
  startup result.
- `install-verification-green`: real signed Antigravity ACP in this enterprise
  environment remained installed/selectable, reported
  `operating_system_policy`, attempted one initialize, and left zero owned
  processes or native dialogs.
- `gui-clean-red`: baseline selection remained enabled, but no startup error UI
  appeared after the actual failed probe.
- `gui-candidate-004c`: four packaged lifecycle cases passed with zero skips.
  Both screenshots were inspected; the error/explicit retry and recovered model
  picker are visible without assertion dialogs or onboarding overlays.
- `publish-validation\e2e quote's workspace`: final reviewed-head E2E passed
  four cases with zero failures/skips, using paths with spaces and an apostrophe.
  The fresh `startup-failed.png` and `startup-recovered.png` were inspected:
  readable failure panel, enabled agent selection, visible retry and recovered
  model picker; no assertion dialog, onboarding overlay or clipped main UI.
- `tests\review-round-one-cpp.log`: all 17 AcpModelUtilsTests passed. The dev
  product and test trees are identical to the final publish head.
- `settings-localization.json`: 93 checks passed across all 18 RESW locales.
- `all-startup-regressions.log`: 26 startup-focused tests passed.
- `history-lifecycle-budget.log`: 23 history tests passed; one opt-in live
  provider test was not invoked by that filter.
- `publish-validation\review-head-real-wsl.log`: explicit Ubuntu-24.04
  process-group recovery/host-exit cleanup passed at the final publish head.
- `publish-validation\review-head-cli-integration.log`: CLI machine-readable
  missing-master failure contract passed at the final publish head.
- `publish-validation\review-head-clippy.log`: all-target Clippy passed at the
  final publish head, with repository warnings.
- `publish-validation\review-head-full-unit.log`: 2,901 passed, two failed,
  five existing opt-in cases ignored. `tests\clipboard-baseline-control.log`
  reproduced the identical clipboard setup failures with the unchanged main
  test binary. This is an environment limitation, not a passing full-suite
  result; no test was weakened or skipped.
- `publish-validation\enterprise-antigravity-review\assertions.json`: final
  installed package reported one attempted/created ACP process, dependency
  present, startup unverified, OS-policy failure, selectable provider, no native
  dialogs, no owned survivors and no remaining ACP processes.
- `reports\release-full-review-head.md` and
  `reports\release-incremental-review-head.md`: C450-C453 all checked.
  The aggregate count includes existing checked items; only four E2E cases were
  newly run.

Normal Dev registration was blocked by the OS activatable-class collector.
Validation uses the isolated PFN
`IntelligentTerminal.Worktree.acplife261019_rd9vj3e6a2mbr`, protocol CLSID
`5e300f1b-09f9-41c5-a12b-e3550345b709`, and immutable layouts under
`C:\ado\intelligent-terminal-private-layouts\acplife261019`.
The normal Dev/Store packages and current-chat process were not removed or stopped.

Two local-only validation corrections must stay out of the feature commit:
the private manifest needs the normal disabled `StartTerminalOnLoginTask`
declaration to avoid a Settings assertion, and the local harness proves exclusive
private COM registration through ClassIndex when unrelated stale package
registrations have no readable manifest. The check still rejects other owners
and classic COM collisions. Temporary MainPage HRESULT instrumentation was
removed and its original byte hash verified. Private branding substitutions
have been restored in both worktrees; their originals and the validated private
manifest are retained in `private-package`.

### Implementation start

2026-10-19: refactoring resumed after #1173 merged. The implementation baseline
is `f643ba403f3c2050b0012ec1a5e31924fee918d8`, including #1169 and #1173.
The dev merge commit is `b0e2c56f0bc548a9a474b3910f880e7a36f4b263`; the original
publish worktree is back on the refactor publish branch and fast-forwarded to
the same main baseline. The three pending refactor files were preserved
byte-for-byte during the merge.

The descendant regression was RED again on this latest baseline, then GREEN
after introducing shared `AgentProcess` ownership across master and probes.
Windows jobs and WSL process groups now share the formerly history-only
containment implementation. The first implementation slice passed all 2883
WTA unit tests and one CLI integration test; the existing opt-in WSL process
cleanup test also passed against Ubuntu-24.04 with no provider/model request.
The next slice at that stage was shared startup admission/retry and installation
verification regressions; those are now implemented and validated above.

### Completed urgent minimal hotfix

2026-10-19: the user prioritized an urgent minimal hotfix before the lifecycle
refactor. PR `microsoft/intelligent-terminal#1173` is published from
`dev/xiaomgao/antigravity-background-startup-hotfix-20261019`, commit
`03752d353e3b22985cbbd625bc97175bcb6d704e`. Copilot reviewed this exact head and
reported no open findings, recommending approval; its review is COMMENTED, not
the required human approval. At publication, remote merge gates were blocked: the Native
WinUI Accessibility Review agent failed in "Prepare trusted accessibility
evidence" with "Directory does not exist: /tmp/gh-aw/mcp-logs" (exit 2), and human
review was required. This workflow failure
was recorded on the PR; no unrelated workflow changes were added to the hotfix.
The user subsequently merged #1173 at 13:33 +08:00 as
`f643ba403f3c2050b0012ec1a5e31924fee918d8`. The hotfix branch is preserved, but
the publish worktree has been switched back to the refactor publish branch.

The minimal change excludes Antigravity from automatic sidebar history startup
before availability probing, using the existing Gemini behavior. Explicit
selection/configured use and availability reporting remain supported. It does
not include lifecycle refactoring, general retry limits, descendant cleanup, or
installation-time verification.

An offline private npm fixture produced ordinary `agy`, `agy.cmd`, and `agy.ps1`
launchers. Baseline marked ACP bootstrap available, downloaded the real pinned
ACP distribution, and launched ACP twice in 60 seconds. Both npm-only and
already-cached scenarios produced zero ACP starts on the hotfix over 50 seconds.
Both were repeated against the exact committed binary:
`368283ED6BF9E975CB7C1B63A9865831C34791EDD6A20A2EABFF2E983EABE1C5`.
The full WTA run passed 2882 unit tests plus one CLI integration test, with five
existing opt-in tests ignored. Evidence is under the `hotfix` subdirectory of
the evidence root.

The refactor's documentation and descendant regression were preserved in this
dev worktree; no refactor code was included in the hotfix PR.

2026-10-19 12:08 +08:00: implementation was authorized. Baseline remains
`a73ca6b8aa76acec1568465a445f59e8409725f8` after a fresh fetch. The user was
unavailable for the package selector form; use its recommended Dev default for
unshipped-code validation, explicitly pinned, never Store or Auto.

### Completed investigation

2026-10-19: investigation completed before implementation approval.
RED reproduced with the unchanged latest-main WTA
master and real signed ACP 1.4.0. The 180-second native-dialog run produced three
automatic ACP attempts, the reporter's exact Bad Image / ucrtbase.dll /
0xc0e90002 error, and two orphaned ACP descendants after their direct parents
were terminated by initialization timeout. Four ACP processes were alive at the
end of observation. The safety job then removed every owned process.

Three queued CSRSS-owned error dialogs survived process cleanup. They were
acknowledged through their verified OK controls, restricted to this run's three
recorded extraction directories; no CSRSS process was stopped. Final dialog
cleanup passed at 11:48:35 +08:00. Those three temporary extraction directories
were subsequently removed after verifying their exact paths, creation instants,
and absence of reparse points. The pinned installer cache and diagnostic
evidence remain available under the ignored evidence root.

At that investigation stop, no product fix or regression test had been written.
After the requested reproduction succeeded, the optional full C++ package build was stopped through
its exact tool session, `antigravity-terminal-build-restored`, rather than
continuing into a GUI deployment. Its partial output was not deployed. The
verified reproduction uses the exact-main WTA binary, not a newly launched
Terminal GUI package.

## Scope And Contract

- Observe actual startup and native error dialogs, not simulated failures.
- Distinguish download/install failure from executable startup failure and ACP
  protocol failure.
- Confirmed clarification from the user: the enterprise restriction is local
  Windows application control/DLL loading, not network connectivity. Do not
  investigate proxies, firewall rules, or network-policy bypasses as the cause
  of this report.
- Distinguish cumulative process launches from simultaneously live or orphaned
  processes.
- Confirm whether startup history discovery alone is sufficient, without a
  prompt or explicit Antigravity selection.
- Determine whether retry timing and process cleanup explain the accumulation.
- Preserve the current chat, existing app instances, settings, authentication,
  provider configuration, and enterprise security controls.
- Implement the approved common lifecycle contract, deterministic regression
  coverage and release metadata; validate exact publish binaries and prepare
  the PR with reviewed evidence.
- Include the subsequently approved installation-time startup check in the
  common lifecycle. Preserve the agent's selectability when installation
  succeeds but the check fails.

## Preserved Invariants And Guardrails

- Only creation-owned process IDs with matching creation time/path may be
  stopped. Never stop processes by executable name.
- The current chat's Copilot and Windows Terminal ancestors are excluded from
  all cleanup.
- Never uninstall any existing package or replace another worktree's layout.
- Never disable application control, code integrity, antivirus, trust checks,
  permission prompts, or signature verification.
- Never change TEMP to evade the enterprise DLL-loading restriction.
- Never log or copy credentials, account IDs, unrelated prompts, or full user
  configuration.
- No model request is required; inference/token budget is zero.
- Initial runtime budget: 180 seconds per reproduction, at most 6 observed ACP
  attempts or 20 simultaneous creation-owned descendants, whichever comes first.
  A smaller run may stop once the causal symptom is unambiguous.
- Every run uses a fresh artifact directory and records start/end, source SHA,
  executable hashes, process identities, error evidence, and cleanup result.

## Ownership Hypothesis

```text
Intelligent Terminal startup
  -> SharedWta master startup
  -> installed host-agent history discovery
  -> Antigravity CLI-only bootstrap eligibility
  -> prepare pinned ACP distribution
  -> spawn ACP executable and send initialize
  -> enterprise restriction blocks extracted executable dependencies
  -> initialization failure/timeout
  -> cleanup plus discovery/helper retry
  -> repeated error dialogs and possibly retained descendant processes
```

- Owning paths:
  `tools\wta\src\antigravity_acp.rs`,
  `tools\wta\src\master\mod.rs`,
  `tools\wta\src\protocol\acp\spawn.rs`,
  `src\cascadia\TerminalApp\SharedWta.cpp`.
- Owning abstractions: ACP dependency preparation, master agent pool/history
  discovery, and spawned-process lifetime.
- Confirmed diagnosis for the bounded master reproduction: installation
  succeeds; the enterprise DLL block holds a PyInstaller child in a native
  error-dialog path; WTA times out initialize and terminates only the direct
  child; automatic discovery subsequently launches a replacement while the
  old descendant remains alive.
- Cheapest discriminating checks: one real ACP initialize attempt with
  PID/parent-PID and native dialog capture; then the unchanged baseline master
  with a private pipe and only Antigravity eligible for automatic discovery.
- Source observation: history startup retries have delays of 5, 10, 20, 40,
  then 60 seconds, with a five-second refresh timer. Do not describe the code as
  an immediate tight retry loop without runtime evidence.
- Nearest existing tests:
  `tools\wta\src\master\tests.rs`,
  `tools\wta\src\antigravity_acp.rs`,
  `test\e2e\tests\Feature.AntigravityProvider.Tests.ps1`.

## External Evidence And Environment

- The external issue reports a Windows packaging/application-control problem:
  `agy_acp_server.exe` extracts executable dependencies such as `python310.dll`
  into `%TEMP%\_MEI*`, and enterprise policy can reject loading them before ACP
  initialize responds. It is not, by itself, evidence of a network firewall
  blocking the ACP protocol.
- The issue was read on 2026-10-19. Its reported ordinary CLI version is
  `1.1.24`; do not confuse that with this machine's version.
- Local ordinary CLI: `agy --version` returned `1.2.8`.
- Baseline's managed ACP version: `1.4.0` for Windows x64.
- Baseline's Windows x64 archive SHA-256:
  `d737aad2d6388e071bc907db415c129a256915d275f436e68f10dcdf6af896da`.
- Existing package/source layouts are not assumed to match latest main.
- Reporter screenshot supplied during investigation:
  `antigravity-acp-error.png`. The visible native dialog is
  `agy_acp_server.exe - Bad Image`, names `%TEMP%\_MEI...\ucrtbase.dll`, and
  reports `0xc0e90002`. Task Manager shows many concurrent `agy_acp_server.exe`
  rows. Its `Background processes (417)` is a total, not a verified count of
  Antigravity processes. Do not reproduce the screenshot's account-specific path
  in committed evidence.
- Installed server SHA-256:
  `2AA1897144A60AAA1B8B54FC0595511AC6E6CE0CF2A7B3166CB1D0E4D4005AB8`;
  Authenticode status `Valid`.
- Exact-baseline WTA SHA-256:
  `A070C019969F85997CB365B14ADAE1503002F2A7318EEFF9F4D4BCB977E9C403`.
- Installed Windows SDK `ntstatus.h` identifies `0xC0E90002` as
  `STATUS_SYSTEM_INTEGRITY_POLICY_VIOLATION`, with the message that Application
  Control blocked the file. Matching CodeIntegrity 3033/3077 events corroborate
  the runtime failure.

## Reproduction And RED Oracle

1. Build the unchanged exact baseline with the explicit Windows target.
2. Record executable identity and real ACP dependency installation status.
3. Run a single bounded ACP startup; capture its stderr, response/absence of
   response, error dialog and descendant lifetimes.
4. Reproduce automatic discovery with the unchanged baseline product path,
   recording every relevant PID, parent PID, creation time, and retry timestamp.
5. If a packaged GUI run is used, record exact package identity and prove it
   cannot collide with any already-installed app or the current chat.
6. Stop only the owned processes and verify cleanup. Record any survivors
   rather than declaring cleanup successful.

- RED oracle: real repeated ACP startup failure plus recurring native dialogs
  and/or growth in concurrently live owned descendants after failed attempts.
- Expected failure layer: provider startup before a successful ACP initialize.
- Not sufficient: a failed download, a fake provider, a mock rejection, a unit
  test alone, or hundreds of deliberately launched processes.
- Large-scale accumulation is inferred only if bounded measurements and source
  lifetime analysis support it; do not intentionally create hundreds of processes.

## Validation Matrix

| Layer | Command or method | Result |
| --- | --- | --- |
| Exact source | Fetch `origin/main`, create two worktrees | Both created at the recorded SHA |
| CLI identity | `agy --version` | `1.2.8` |
| Baseline WTA | `cargo build --locked --target x86_64-pc-windows-msvc --manifest-path tools\wta\Cargo.toml` | Passed; exit 0; 3m23s |
| ACP preparation | `wta.exe install-acp --agent-id antigravity`, isolated LOCALAPPDATA only | Passed; exit 0; signed pinned pair installed |
| Standalone ACP | Real initialize, 12-second observation | Inconclusive: stopped before approximately 20-second extraction completed |
| Master, inherited error mode | Real baseline master, 180 seconds, no helper/prompt | Five starts, four completed policy-blocked failures; no dialog or retained ACP child |
| Master, default error mode | Same baseline/master/cache/policy, 180 seconds | RED: exact native error plus two orphaned descendants and another active attempt |
| Visual evidence | Targeted native error-window capture, manually inspected | Matches reporter; ucrtbase.dll and 0xc0e90002 visible |
| Process cleanup | Safety job close plus PID/creation-time checks | Passed; no surviving owned ACP processes |
| Dialog cleanup | Acknowledge only errors naming this run's extraction paths | Passed; three queued dialogs removed, CSRSS untouched |
| Temporary extraction cleanup | Exact recorded paths, creation instants and reparse-point checks | Passed; three owned directories removed |
| Exact GUI package | Baseline package build after missing NuGet restore | Deliberately stopped after bounded reproduction; not passed, deployed or launched |
| Focused GREEN | Startup, history, native parser and process ownership regressions | Passed; review regressions included in final-head full units |
| Full Rust units | `cargo test --locked --target x86_64-pc-windows-msvc --manifest-path tools\wta\Cargo.toml --bin wta` at `7803dd855` | 2,902 passed; two baseline clipboard setup failures; five opt-in ignored |
| CLI integration | Same Cargo target/manifest with `--test sessions_cli` | One passed on dev WTA source identical to `7803dd855` |
| Real WSL ownership | Same Cargo target/manifest with `--bin wta history_wsl_group_reclaims_owned_work_on_recovery_and_host_exit -- --ignored --nocapture`, `WTA_TEST_WSL_DISTRO=Ubuntu-24.04` | One passed on dev WTA source identical to `7803dd855`; no model request |
| Native parser/model tests | `TE.exe Terminal.App.Unit.Tests.dll /name:*AcpModelUtilsTests*` | 17 passed after rebuilding the exact `ba61c2929` solution test target |
| Static analysis | `cargo clippy --locked --target x86_64-pc-windows-msvc --manifest-path tools\wta\Cargo.toml --all-targets` | Passed on dev WTA source identical to `7803dd855`; existing warnings |
| Localized primary error RED | Focused C450 against `1b096102`/0.8.0.16, before product edit | Expected UIA assertion failed on the appended raw diagnostic suffix |
| Packaged E2E | `Invoke-Pester -Configuration $config`, `Feature.AcpLifecycle.Tests.ps1`, explicit PFN/App/WTA hashes | Four passed with exact `7803dd855` WTA and unchanged `ba61c2929`/0.8.0.18 native carrier; zero failures/skips; fresh screenshots inspected |
| Real enterprise installation check | Private package CLI alias, bounded local runner, no model request | Expected failure contract passed; one launch, selectable, zero dialogs/survivors |
| Release report mapping | `New-ReleaseReport.ps1` and `Update-ReleaseReport.ps1` against final XML | C450-C453 checked in both reports |

### Previous Exact Publish Identity (`ba61c2929`, before WTA-only refresh)

- Source commit: `ba61c2929c3494d380fe2aca30ce83989884afe3`.
- Package: `IntelligentTerminal.Worktree.acplife261019_rd9vj3e6a2mbr`,
  version `0.8.0.18`.
- Installed immutable layout:
  `C:\ado\intelligent-terminal-private-layouts\acplife261019\a11y-publish-008`.
- Build: `MSBuild.exe OpenConsole.slnx /t:Terminal\CascadiaPackage /p:Configuration=Debug /p:Platform=x64 /p:WindowsTerminalBranding=Dev /p:AppxSymbolPackageEnabled=false /p:CL_MPCount=2 /m:4 /v:minimal /nologo`.
- Build/deployment journal: `private-package\a11y-publish-008-journal.json`.
- Receipt: `private-package\a11y-publish-008-deployed.json`.
- `WindowsTerminal.exe` SHA-256:
  `55639119A8212828A25AC7DA401DC65CFBEA2F710A689E4863FBE01945836B09`.
- `TerminalApp.dll` SHA-256:
  `FE2A922611D5B68EE53257025F3F7DD54E0AA60BDC69A325F5B7611CB54EAA5F`.
- `wta.exe` SHA-256:
  `D47311B1A4EA160E02C127948807FDA80E7480175AE1DE7D851A76A5B95066E2`.
- `Microsoft.Terminal.Settings.Editor.dll` SHA-256:
  `557F96F955D32D2B66D7360975F4AABDF3B837B0AEB57187CCDA3D23555F0996`.
- The receipt explicitly records dirty identity-only build inputs. Normal
  manifest, window-class prefix and protocol CLSID originals were restored
  byte-exact afterward; the publish worktree is clean.
- The dev harness supplies test source and its local-only COM-registration
  correction; it does not substitute a dev WTA or Terminal binary.
- Package aliases are execution reparse entries. The bounded runner hashes the
  installed binary and verifies the launched process image instead of trying to
  hash the zero-length alias.
- Final source-package check:
  `publish-validation\a11y-final-package-identity.json` and
  `publish-validation\a11y-settings-editor-identity.json` independently compare
  actual MSBuild staging in `src\cascadia\CascadiaPackage\bin\x64\Debug`, the
  immutable MSIX entries and installed binaries. Current explicit-target WTA
  also matches. Every comparison passed. MSIX SHA-256:
  `B902F74B267688BC10ADA0DE47A436D27FB3149713D8CE12B96EF532915A5264`.
- Do not confuse that staging directory with its stale `AppX` subdirectory from
  the earlier hotfix. The latter was not installed or used for this acceptance.
  The earlier `1b096102` acceptance remained pinned to its F600D200 MSIX binary
  after later Cargo commands regenerated mutable output. The final 0.8.0.18
  acceptance instead pins D47311B1 across current Cargo output, package staging,
  MSIX and installed layout.

## Implementation Record

At implementation approval, no product changes had been made. Provider settings,
enterprise policy and authentication remain protected. Local-only diagnostic
orchestration and this investigation record remain separate from publishable
product code, tests and documentation.

The implementation now shares provider preparation, typed failure reporting,
startup admission and cleanup policy across runtime, history and model probes.
Each persistent target has at most three automatic failed starts; successful
initialize resets failures, explicit retry begins a new epoch, and an
unconfirmed cleanup failure blocks replacement. Pool-generation counters are
not persistent target identities. A queued helper has a conservative 1,920s
overall bound, while the provider initialize timer remains 60s.

Installation verification performs one initialize-only exchange and cleanup.
Settings reuses initialization for model discovery, preserves agent selection
on failure, and offers explicit retry. Native loader UI suppression does not
change enterprise policy enforcement. No ordinary Antigravity npm CLI is
installed, and #1173's unsolicited-background-startup exclusion remains intact.

## Confirmed Findings

### Installation is not the failing stage

The baseline installer downloaded and verified the official ACP 1.4.0 pair
successfully. The server itself has a valid Authenticode signature. The failure
occurs later, when the server extracts and loads its bundled DLLs. CodeIntegrity
records the load rejection. Reinstalling the same successful archive is not a
demonstrated remedy.

### Startup discovery can trigger this without selecting Antigravity

`tools\wta\src\master\mod.rs:5243-5245` starts host history discovery
unconditionally at master startup.
`tools\wta\src\master\mod.rs:7870-7874` includes both launch-ready agents and
CLI-only installations eligible for ACP bootstrap. The main startup connection
therefore attempts Antigravity even without a helper, chat prompt, or explicit
Antigravity selection. The local master run had no connected helper and sent no
model request.

### Timeout cleanup owns the bootloader, not its complete process tree

`tools\wta\src\master\mod.rs:6085-6129` waits up to 60 seconds for initialize and
calls `finish_failed_startup` on failure/timeout.
`tools\wta\src\protocol\acp\spawn.rs:147-163` calls `child.start_kill()`, drains
stderr for at most 250 ms, and does not retire descendants.
The main pooled connection takes the non-history branch at
`tools\wta\src\protocol\acp\spawn.rs:464-476`; that branch has no per-provider
process-tree job. The dedicated history-worker job is not used for this
initial shared-pool connection.

The outer Terminal master job in
`src\cascadia\TerminalApp\SharedWta.cpp:964-995` provides whole-master shutdown
containment, but it stays open while Terminal remains running. It therefore
does not clean up each failed provider attempt.

### Backoff slows the accumulation but does not stop it

`tools\wta\src\master\mod.rs:1915-1924` returns 5, 10, 20, 40, then 60 seconds
after failures. The failure counter saturates at five for the backoff calculation;
it is not a maximum-attempt count. The five-second refresh loop continues
requesting eligible failed agents at
`tools\wta\src\master\mod.rs:7643-7652` and `7833-7845`.

| Attempt | Parent PID | Start, seconds | Parent last observed | Descendant PID | First observed without parent | Descendant alive at 180s |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | 9020 | 1.001 | 59.876 | 23720 | 60.449 | Yes |
| 2 | 27544 | 70.433 | 129.825 | 23128 | 130.416 | Yes |
| 3 | 24996 | 145.473 | Still alive | 23360 | Parent still alive | Yes |

At the observation boundary, four ACP processes were alive: two orphaned
descendants, the third attempt's parent, and its descendant. Master and its
console host bring the safety job total to six. The experiment deliberately did
not create hundreds of processes; unbounded eventual accumulation is supported
by repeated retained descendants and the absence of a retry stop condition.

### Error presentation is a necessary reproduction precondition

The PowerShell runner inherited error mode `0x8001`, including
`SEM_FAILCRITICALERRORS`. In that mode, the same DLL block returned errors to
PyInstaller instead of waiting for dialog acknowledgment. The headless control
therefore reproduced repeated initialization failures but not retained
descendants.

Creating only the diagnostic root with `CREATE_DEFAULT_ERROR_MODE` reproduced
the native Bad Image dialog and orphan growth. This flag restores ordinary
Windows error presentation; it does not disable or alter application-control
enforcement. The original policy remained active and blocked the same DLL.

The visible hard-error dialog is owned by `csrss.exe`, not by the ACP PID.
Scoped process cleanup alone consequently left three queued dialogs. Cleanup
confirmed each dialog's title, error code, and recorded extraction directory,
then acknowledged the actual OK button (native control ID 2). An initial
assumption that OK used ID 1 did not dismiss anything; the failed cleanup
receipt is retained rather than represented as a pass.

## Reproduction Commands

Run from the dev worktree in a fresh PowerShell process. Evidence directories
must not already exist.

```powershell
$base = Join-Path $PWD 'artifacts\antigravity-acp-init-storm-20261019'
$wta = 'C:\ado\intelligent-terminal-antigravity-acp-init-storm-20261019-publish\tools\wta\target\x86_64-pc-windows-msvc\debug\wta.exe'
.\local-investigation\Invoke-BoundedAcpRepro.ps1 `
    -Mode Master -Executable $wta `
    -OutputDirectory (Join-Path $base 'master-default-error-mode-002') `
    -LocalStateRoot (Join-Path $base 'runtime-state') `
    -Seconds 180 -DefaultErrorMode
```

The harness uses an isolated WTA cache/rendezvous root, a private master pipe,
an Antigravity-only allowlist, and `--no-session-management` to avoid changing
unrelated CLI hooks. It supplies an unregistered control CLSID so it cannot
activate or manipulate another Terminal. The resulting eight bounded wtcli
connection failures are isolation-related diagnostics, not the ACP defect.
TEMP, enterprise policy, provider login and the agent's executable are unchanged.

## User-Requested Direction For Discussion

The user clarified that the eventual fix must apply to ACP startup failures
regardless of cause, not only Antigravity's enterprise DLL block:

- The product defect to fix is Intelligent Terminal's unbounded automatic
  startup/retry and retained failed processes. An external policy rejection is
  an allowed failure; a continuing process/dialog storm is not.
- Do not treat bypassing or weakening enterprise policy as a product remedy.
- Bound automatic retries instead of retrying indefinitely.
- Ensure automatic startup entry points cannot individually reset or evade the
  effective retry limit and continue creating processes after it is exhausted.
- Promptly clean up the failed attempt's ACP process tree.
- Preserve other agents and successful sessions while containing that failure.
- Use three total automatic startup attempts, including the first, for the
  approved implementation. Do not interpret this as three additional retries.
- Implementation was approved after the architecture discussion at 12:08:35
  +08:00. The installation-time startup check was subsequently approved, with
  installed providers remaining selectable after verification failure. Both
  belong to the refactor currently paused behind the urgent hotfix.

Before implementation, agree on the attempt-count definition, retry-budget
scope/reset conditions, and explicit manual-retry behavior. Those decisions must
cover background discovery, foreground/helper startup, and other relevant ACP
startup entry points without killing unrelated sessions or shared providers.

### Approved implementation contract

- Separate executable/dependency detection, preparation, process creation,
  protocol initialization, authentication, and session readiness.
- Reuse the existing agent metadata and usage-style provider adapter boundary.
  Provider adapters supply detection/preparation/launch differences; common code
  owns retry admission, timeouts, failure recording, and process-tree lifetime.
- Automatic requests for the same effective provider/source/launch configuration
  must share the three-attempt limit. Refresh and pane selection do not create a
  fresh failure budget. An explicit retry is a distinct user action.
- Cleanup is a barrier, not best effort: do not launch a replacement while the
  previous attempt's owned tree is still alive or its exit is unconfirmed.
- Keep session-local protocol/authentication failures distinct from failed
  provider-process startup, preserving healthy shared sessions.
- Preserve safe structured failure phase/reason/attempt metadata across WTA
  boundaries without forwarding credentials or raw provider configuration.
- Keep detection side-effect-free and preparation idempotent; never treat an
  already-installed but blocked executable as an instruction to reinstall it.
- Proposed first RED checks: repeatedly request failing startup beyond three
  attempts; fail a launcher that leaves a descendant; verify healthy sibling
  providers are unaffected and ordinary refresh cannot reset admission.

### Installation-time startup check: investigation and approval

The user asked about one startup attempt after installation, explicitly as a
proposal rather than an implementation decision, then approved including it in
this PR. The clarification concerns user-side latency/resources, not development
cost. The approved design keeps an installed agent selectable after failed
verification, allowing explicit agent-pane retry.

- The existing background connection already runs prepare, spawn and initialize.
  Settings runs dependency preparation followed by its model probe. The helper
  install-completion handler reconnects after detecting the prepared executable.
  The standalone `install-acp` command itself only prepares the dependency.
- An initialize-only check does not submit a model prompt. ACP initialization
  negotiates protocol/capabilities; provider-specific initialization may still
  perform filesystem or network work, so it cannot be promised to have zero
  external activity for every provider.
- Local observed preparation took approximately 27 seconds. The blocked
  Antigravity executable reached DLL loading after approximately 20 seconds;
  a native error dialog then held startup until the 60-second initialize
  deadline. These are failure-environment measurements, not successful startup
  benchmarks for all providers.
- Reusing the normal first startup adds no duplicate cold launch. A disposable
  post-install probe followed by a second normal startup adds a complete extra
  launch and cleanup cycle.
- If selected later, report "installed, startup check failed" separately from
  installation failure and authentication requirements. A check must be bounded,
  own its entire process tree, and participate in the shared startup budget.
- A successful installation-time check never removes runtime retry/cleanup
  protections. The approved check is one bounded attempt and does not submit a
  model prompt or initiate login.
- Do not collapse check failure into `AgentInstallOutcome::Failed`: preparation,
  verification, selectability and live connection state are independent facts.
- Selection remains gated by policy, not by a cached successful initialization.
  A failed check must not hide the provider or revert the user's selection.
- The publishable contract is recorded in
  `doc\specs\acp-provider-lifecycle.md`.

### First deterministic RED

`failed_acp_startup_reclaims_the_owned_descendant_before_returning` compiled and
failed at the expected behavioral oracle on the unchanged startup implementation:
the direct child was stopped, but the owned descendant remained live when
cleanup returned. The independent test safety job reclaimed all fixture
processes before the RED assertion. The run reported 0 passed, 1 failed,
2887 filtered out; exit 101.

Evidence: `artifacts\antigravity-acp-init-storm-20261019\tests\owned-descendant-red.json`
and the adjacent log. No product implementation preceded this RED check.

## Evidence Limits

- The initial RED is an exact-main binary reproduction through the real automatic master
  discovery and process-launch path, not a mocked agent or injected state.
- That first bounded RED was not a newly built GUI launch. Later validation
  includes clean baseline GUI RED and final-head private-package GREEN.
- The isolated installer command exercises the same production preparation
  code, but the bounded master run reused that freshly installed managed pair.
- No assertion is made that the reporter had exactly 417 ACP processes.
- Real acceptance proves safe failure handling under the existing enterprise
  policy, not successful Antigravity operation despite that policy.
- The first packaged acceptance sampling attempt did not see child PIDs in the
  outer observer's Job. Its artifact remains failed, not relabeled. The final
  oracle combines the product process-creation event, structured attempt count,
  actual launched image/hash and absence of survivors/dialogs.
- Two unchanged-baseline clipboard tests still fail; full local Rust validation
  must not be reported as wholly green.

## Review Triage

Round one: review `5575932105`, `2026-10-19T18:28:23Z`, publish head
`7301970aa3c40dfd07e1f3a71f541ab19d5b7912`.
All findings were accepted and fixed in dev `1920ba309...`, directly cherry-picked
and pushed as publish `1b0961026720dedb9debe3293141abf2f0993197`.

| Finding | Resolution | Evidence |
| --- | --- | --- |
| Fixture paths with spaces | Encoded PowerShell fixture command preserves arguments, including apostrophes | Final E2E in `e2e quote's workspace`, four passed |
| Probe/runtime target mismatch | Built-in live-model launch identity matches runtime; launch-selected models stay distinguished | Native parser/model tests 17 passed; Rust identity tests pass |
| External explicit retry leaves stopped workers closed | Persisted retry epochs reopen discovery/history and reject stale-cycle failures | Full Rust review regression log |
| Custom target identity changes after restart | Stable binding digest replaces process-local generation in admission keys | Full Rust review regression log |
| Helper timeout includes admission waiting | Separate overall helper bound covers queue/preparation/initialize/cleanup; initialize remains 60s | Deadline and admission regression tests |
| Cancelled WSL receipt can lose process ownership | Retain the process/lease before awaiting receipt acquisition | Rust cancellation regressions and final real WSL cleanup test |
| History post-initialize cleanup failure is not global | Retain registry/key and persist quarantine after successful initialization | Rust history cleanup-quarantine regressions |
| Two spelling findings | Rename the model-catalog test identifier | Remote spelling checks pass |

Every first-round thread has a pushed-SHA reply and is resolved. Final cleanup
found no outdated unresolved Copilot threads.

Round two: review `5576610223`, `2026-10-19T19:25:32Z`, that round's exact publish
head `1b0961026720dedb9debe3293141abf2f0993197`. It reports "No open issues",
seven prior correctness findings resolved, and no suppressed findings.
The review still requests human attention to interprocess accounting and
Windows/WSL cleanup. Its statement that exact-head validation is pending came
from the older PR body; the evidence above now supplies that validation.

The legacy helper's `NoNewComments` regex does not match `ccr-overview-v2`;
`ReviewAtHead=true`, zero open threads, and `Converged=false` are recorded
without overriding the check. Remaining open gates are human approval,
review-infrastructure failures and the explicit local clipboard limitation.

Ready-event gate triage at `2026-10-19T20:27:11Z`:

| Gate | Exact evidence | Disposition |
| --- | --- | --- |
| Security | Run `39313414048`, completed success at `19:56:16Z` | Passed; no high-confidence boundary regression or proposed repair reported |
| Localization | Run `39313413952`, completed success at `20:10:52Z` | Passed |
| Native smoke | Job `118274941943`, completed success at `20:21:20Z`, unchanged publish head | Passed; retain this attempt's result independently of the retry |
| Native generated review | Run `39313414119` attempt 1, job `118269596388`, exit 2 | Blocked: `review_checks` must cite every classified changed file |
| Native retry | One `gh run rerun 39313414119 --repo microsoft/intelligent-terminal --failed` at `20:25:34Z`; attempt 2, job `118288546700` running | Await result; retry authorization consumed |
| Performance | Controller `39313413985` attempt 2 failed at `20:26:28Z`; fresh worker `39316696186` failed at `20:26:09Z` | Repeated 15-minute execution timeout; summary not delivered; not a completed assessment |

The native unpublished report had no HIGH/MEDIUM/LOW findings or repair commits,
but requested manual Windows/Narrator checks of error announcements, diagnostic
suffixes and duplicate retry announcements. Because its report contract failed,
it is not counted as a successful accessibility review. Performance's missing
output cannot establish that unpublished findings are absent. No new inline
findings, repair commits or publish-head changes were observed.

Accessibility attempt-2 follow-up: the latest artifact (`agent`, uploaded
`2026-10-19T20:40:24Z`) did contain a HIGH finding about raw diagnostic suffixes
in the primary message. The earlier no-findings description above applies only
to attempt 1. Parent retrieved the latest artifact and acted on its actual
content rather than equating failed publication with no findings.

- Accepted HIGH: localized primary text no longer appends phase/reason/count or
  GUID data. Diagnostics remain in safe WIL logging and structured CLI output.
  Standard InfoBar.Message/ActionButton replace custom content, and message
  notification precedes opening.
- RED: `publish-validation\a11y-primary-red` at prior package 0.8.0.16 failed
  only the exact localized UIA-message assertion. The RED screenshot was
  inspected; setup and selection were valid.
- GREEN: candidate `a11y-candidate-007b` and final
  `publish-validation\a11y publish quote's path` both passed all four cases.
  Both sets of fresh screenshots were inspected. Native tests passed 17/17;
  full exact-head Rust retained the two baseline clipboard failures.
- First candidate compilation rejected narrow strings passed to WIL's wide
  formatting with `%s`; corrected to `%hs`, without weakening warnings.
  Failed `a11y-candidate-007` journal/log remains preserved.
- Dev product commit: `2ef93e5bc800541858d8b3defc97b380d52fe44b`.
  Direct publish cherry-pick: `ba61c2929c3494d380fe2aca30ce83989884afe3`.
  Dev was pushed before the validated publish head.
- MEDIUM runtime request: manual Narrator timing/uniqueness across preparation,
  initialization and cleanup is not claimed. Automated UIA proves localized
  startup-failure content and retry, not audible screen-reader output.
- LOW informational item: all 18 locale resources were already translated;
  no resource values/comments needed changing.
- Current-head Copilot review `5578063661` at `ba61c2929...`,
  `2026-10-19T21:35:28Z`, has no open issues or suppressed findings. It requests
  final human scrutiny of interprocess accounting, process cleanup and Narrator
  validation; those are review requests, not newly identified defects. The
  legacy helper still reports `Converged=false` for its unrecognized overview
  format. No prior-head remote outcome is reused as new-head proof.

Gate disposition for `ba61c2929`, after `2026-10-19T22:09:31Z`:

| Gate | Result | Evidence |
| --- | --- | --- |
| Copilot code review | No open/suppressed defects, COMMENTED | Review `5578063661`, exact `ba61c2929` |
| Security | Passed | Run `39327823493`, completed `21:40:00Z` |
| Localization | Passed | Run `39327823671`, completed `21:59:34Z` |
| Spelling and CLA | Passed | Current-head check rollup in `publish-validation\final-pr-gates.json` |
| Native Axe.Windows smoke | Passed at current head | Job `118325185862`, completed `22:09:31Z` |
| Native source review | Failed report publication-policy validation, exit 2 | Run `39327823686`; artifact `12224505501` has empty findings/dismissed/patch arrays, but was not accepted for publication |
| Performance review | Incomplete: 15-minute timeout and missing summary | Controller `39327823591`, worker `39327883500`, artifact `12223514546`; empty output is not a no-findings verdict |
| Human approval | Required | GitHub `MERGEABLE` diff with `BLOCKED` / `REVIEW_REQUIRED` |

The final native smoke was awaited to completion in dedicated shell
`acp-final-native-watch`; its exit 1 describes the overall failed workflow, not
the successful smoke job. The native structured artifact also asks for manual
Narrator/keyboard and contrast/theme/scaling verification. The local real UIA
test and separate successful native smoke supply their actual evidence only;
they do not claim audible Narrator or unexecuted visual matrices.

### Review-tool compatibility and local performance completion

The legacy-format caveats above describe earlier snapshots. The dev-only
compatibility correction now recognizes the actual explicit zero-issue summary
while retaining all other requirements; `Converged=true` was obtained for the
genuinely reviewed `ba61c2929` head, not forced or reused for `7803dd855`.
The product PR excludes this tooling commit.

Local performance reviewer `052e2222-cd88-4fae-b8f3-00f705f3b3c0` reviewed the
full `f643ba403..ba61c2929` product patch and then independently inspected
`ba61c2929..7803dd855`. The MEDIUM no-op history-gate dependency is resolved
and its native regression evidence was inspected. This supplements, rather
than falsifies, the incomplete GitHub performance runs. No measured throughput,
UI-latency distribution or resource-soak improvement is claimed.

The native report's citation failures were reproduced against immutable Git
blobs. The performance worker logs show review work continuing until the
15-minute cutoff, with two denied command attempts; denied requests were not
bypassed and are not asserted to be the sole timeout cause. Both workflows use
trusted base revisions, so changing product-branch workflow files would not
repair their currently executed definitions.

## Local-Only Evidence Inventory

| Artifact | Path | Purpose |
| --- | --- | --- |
| Investigation record | `local-investigation\ANTIGRAVITY-ACP-INITIALIZATION.md` | Scope, baseline, findings and handoff |
| Bounded runner | `local-investigation\Invoke-BoundedAcpRepro.ps1` | Local-only observer and safety containment; not a product patch |
| Dialog cleanup | `local-investigation\Close-ReproErrorDialogs.ps1` | Only acknowledges creation-correlated native errors |
| Build journal/log | `artifacts\antigravity-acp-init-storm-20261019\build` | WTA passed; initial C++ missing dependency failure, restore, deliberately stopped package build |
| Installer receipt | `artifacts\antigravity-acp-init-storm-20261019\install\receipt.json` | Successful real pinned dependency preparation |
| Short direct check | `artifacts\antigravity-acp-init-storm-20261019\direct-001` | Inconclusive short probe; cleanup succeeded |
| Inherited-mode control | `artifacts\antigravity-acp-init-storm-20261019\master-001` | Policy block and retries without native dialogs |
| RED run | `artifacts\antigravity-acp-init-storm-20261019\master-default-error-mode-001` | Native error, exact PIDs, orphan lifetime table, policy events and cleanup |
| RED screenshot | `master-default-error-mode-001\window-10864-17432594.png` under evidence root | Same Bad Image / ucrtbase.dll / 0xc0e90002 as reporter |
| Lifetime analysis | `master-default-error-mode-001\process-lifetimes.json` under evidence root | Two direct parents gone while their descendants remain live |
| Final dialog cleanup | `master-default-error-mode-001\dialog-cleanup.json` under evidence root | Three queued dialogs acknowledged; no system process termination |
| Extraction cleanup | `master-default-error-mode-001\extraction-cleanup.json` under evidence root | Only the three identity-verified temporary directories removed |
| Runtime log | `runtime-state\IntelligentTerminal\logs\wta-main_master.2026-10-19.log` under evidence root | Source-real startup, retry timestamps and initialization failures |
| Clean GUI RED | `gui-clean-red` under evidence root | Baseline selection remains enabled but startup error UI is absent |
| Final package receipt/journal | `private-package\publish-review-006-{deployed,journal}.json` | Final publish source, identity substitutions, immutable installed layout and binary hashes |
| Identity restoration | `private-package\review-006-identity-restored.json` | Only the three known private identity inputs restored byte-exact |
| Final artifact identity | `publish-validation\final-package-staging-identity.json` | Actual MSBuild staging, immutable MSIX entries and installed binaries all match; stale AppX and later Cargo outputs are not substituted |
| Final GUI/protocol/process E2E | `publish-validation\e2e quote's workspace\results.xml` and its capture subdirectory | Four real packaged boundaries, quoted paths, fresh failure/recovery screenshots |
| Final E2E/report assertions | `publish-validation\final-e2e-report-assertions.json` | Four passes, zero failures/skips, visual inspection and C450-C453 |
| Native tests | `tests\review-round-one-cpp.log` | 17 parser/model tests; product/test source equivalence verified |
| Final unit/CLI/WSL/Clippy | `publish-validation\review-head-{full-unit,cli-integration,real-wsl,clippy}.{log,json}` | Exact publish head and individual outcomes; full-unit exit 101 is retained |
| Clipboard baseline controls | `tests\clipboard-baseline-control.log`, `tests\clipboard-environment-recheck.log` | Same two setup failures on unchanged baseline |
| Enterprise final acceptance | `publish-validation\enterprise-antigravity-review\assertions.json` | Final installed WTA hash, one attempt/creation event, OS-policy reason, zero dialogs/survivors |
| Earlier sampling-oracle failure | `publish-validation\enterprise-antigravity` | Outer observer Job did not enumerate ACP children; not a passing artifact |
| Final release reports | `reports\release-{full,incremental}-review-head.md` | C450-C453 checked from final four-case XML; no claim that every aggregate check was rerun |
| Pre-ready accessibility evidence | `publish-validation\ci-accessibility-review-round-two` | Immutable artifact for the earlier run `39308994040`; its agent timed out, and ready-event concurrency later superseded its smoke |
| Remote-head verification | `publish-validation\final-remote-heads.json` | Dev evidence commit `b845d887b6087c424161c0ebeec4108240cb7ccb` and publish `1b096102...` matched their remote refs |
| Latest unpublished native review | `publish-validation\ci-native-attempt-two` | Actual HIGH diagnostic-suffix finding, MEDIUM manual Narrator request and LOW translation confirmation; failed report publication did not erase the finding |
| Accessibility RED | `publish-validation\a11y-primary-red` and `reports\release-a11y-red.md` | Old package fails exact localized UIA text; C450 correctly remains failed, not checked |
| Accessibility candidate | `private-package\a11y-candidate-007b-*`, `publish-validation\a11y-primary-green` | First successful focused build/deployment and four-case GREEN before publication |
| Final accessibility package | `private-package\a11y-publish-008-*`, `publish-validation\a11y-final-package-identity.json`, `a11y-settings-editor-identity.json` | Exact `ba61c2929` build and four binary identities, including the changed owner DLL |
| Final accessible E2E | `publish-validation\a11y publish quote's path`, `a11y-final-e2e-report-assertions.json` | Four passes, quoted paths, exact localized primary UIA text, retained selection/retry, fresh inspected screenshots |
| Final native/Rust tests | `publish-validation\a11y-publish-{native-build,native-tests,full-unit}.log` | Exact-head native 17 passed; Rust 2,901 passed plus two baseline clipboard failures |
| Final enterprise acceptance | `publish-validation\a11y-enterprise-antigravity-verified` | Exact D47311B1 installed WTA, one real OS-policy failure/start, selectable, no dialogs/survivors |
| Preserved log-oracle failure | `publish-validation\a11y-enterprise-antigravity\oracle-failure.json` | Initial observer incorrectly selected wta-probe instead of install-acp's wta-cli log; retained as failed, then a fresh bounded run used the correct log |
| Final accessibility release reports | `reports\release-{full,incremental}-a11y-final.md` | C450-C453 checked from the latest exact-package four-case XML |
| Final current-head gates | `publish-validation\final-pr-gates.json`, `final-native-run.json`, `final-native-watch.log` | All current-head runs terminal; native smoke passed, source report/performance failed, human review required |
| Curated unpublished-review inspections | `publish-validation\final-review-evidence` | Byte-exact copied monitor/inspection records with digest receipt; native findings empty, performance incomplete |
| Original monitor evidence | `C:\ado\intelligent-terminal\artifacts\review-monitor\pr1180-ba61c2929-214119` | Ignored raw artifacts retained; only inert report data inspected, no logged commands executed |
| Accessibility-head remote proof | `publish-validation\a11y-final-remote-heads.json` | Dev evidence `57cfcace0...` and publish `ba61c2929...` matched remote heads before the final gate-note commit |
| Review-format RED/GREEN | `Generated Files\acp-review-followup\review-format-{red,green}.log`, `review-format-tests.json` | Four new failures before correction, 84 passes afterward; unchanged guard behavior |
| Real convergence / desktop diagnosis | `Generated Files\acp-review-followup\{current-head-convergence,windows-powershell-convergence,desktop-prerequisites}.json` | Genuine `ba61c2929` convergence and native clipboard access denied without data access |
| Native report diagnosis | `Generated Files\acp-review-followup\native-citation-diagnosis.json` | 18 missing citations and five bad immutable references; original rejected report not relabeled |
| Retry/history RED/GREEN | `Generated Files\acp-review-followup\retry-history-{red-native-binding,green}.log` | Actual pending ACP history RPC blocks old initialize; corrected initialize completes while history remains pending |
| Excluded fixture failure | `Generated Files\acp-review-followup\retry-history-red.log` | Initial legacy/native binding mismatch failed identity assertion; not accepted regression evidence |
| New WTA tests | `Generated Files\acp-review-followup\retry-history-{full-suite,cli-integration,real-wsl,clippy}.log`, `retry-publish-full-unit.*` | 2,902 passes plus two baseline clipboard failures; CLI/WSL/Clippy pass; exact publish unit provenance |
| WTA-only deployment | `Generated Files\acp-review-followup\retry-publish-{wta-build,deployed}.json`, `retry-carrier-identity.json` | WTA source `7803dd855`, native carrier source `ba61c2929`, no full package deployment |
| Preserved previous executable | `Generated Files\acp-review-followup\wta-ba61c2929-before-refresh.exe` | Exact D47311B1 prior WTA; original carrier MSIX also remains unchanged |
| Final retry-runtime E2E | `Generated Files\acp-review-followup\e2e quote's path`, `retry-final-e2e-report.json`, `reports` | Current WTA 4/4, inspected screenshots, C450-C453 checked |
| Final retry-runtime enterprise acceptance | `Generated Files\acp-review-followup\enterprise-verified\assertions.json` | DE03B85B runtime, one real policy-blocked attempt, selectable, zero dialogs/survivors |
| Current-head final native run | `Generated Files\acp-review-followup\current-native-{run.json,watch.log}` | `7803dd855` smoke passed on October 20 at 00:04:16 UTC; source-report failure kept separate |

## Completed Investigation Checklist

- [x] Fetch latest main and record exact source.
- [x] Create independent dev/publish branches without changing the dirty worktree.
- [x] Fill the template with the investigation-only contract.
- [x] Trace installation, initialization, retries and process ownership.
- [x] Reproduce the actual enterprise failure with bounded process creation.
- [x] Record native dialog/error and process-lifetime evidence.
- [x] Verify runtime cleanup and preservation of unrelated state.
- [x] Record confirmed findings separately from hypotheses.
- [x] Stop for discussion without implementation.

## Implementation And Publication Checklist

- [x] Preserve the merged #1173 emergency background-startup guard.
- [x] Common lifecycle/adapters, three-attempt admission and owned cleanup.
- [x] Installation-time initialize-only verification, errors and retained selection.
- [x] Deterministic RED/GREEN, native parser and review regressions.
- [x] Separate product commits from dev-only orchestration and evidence.
- [x] Push dev then directly cherry-picked publish product commits.
- [x] Build/deploy final publish product source with documented private identity.
- [x] Verify final installed binary identity and real packaged E2E.
- [x] Inspect fresh final-head failure/recovery screenshots.
- [x] Verify real enterprise safe-failure behavior and real WSL containment.
- [x] Credit C450-C453 in full and incremental release reports.
- [x] Address every first-round finding; second review at `1b096102` has no open issues.
- [x] Recover, fix and revalidate the unpublished native HIGH finding with real UIA RED/GREEN.
- [x] Complete supplemental local performance assessment and correct the demonstrated MEDIUM retry/history dependency.
- [x] Repair review-summary format compatibility with strict regression/guard coverage, kept dev-only.
- [ ] Fully green local Rust suite: blocked by two unchanged-baseline clipboard setup failures.
- [x] Current-head Copilot review has no open or suppressed defects and genuine convergence is verified.
- [x] Current-head native Axe smoke completed successfully.
- [ ] All optional remote reviews green: source-report publication, performance and partial security execution remain failed/incomplete.
- [ ] Mandatory non-author merge approval: handed off to the unavailable user; never fabricated or bypassed.
- [x] Push dev-only handoff/runner update and confirm remote heads (`b845d887b...`).
- [x] Record final current-head review/gate disposition without claiming merge readiness.
