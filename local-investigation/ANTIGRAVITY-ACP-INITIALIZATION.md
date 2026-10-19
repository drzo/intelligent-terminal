# Antigravity ACP initialization failure storm: investigation only

## Feature Metadata

- Date: 2026-10-19.
- Summary: investigate automatic Antigravity ACP preparation followed by repeated
  initialization failures, native error dialogs, and accumulating processes.
- User-visible goal: reproduce and explain the reported startup failure, then
  stop for discussion before choosing or implementing a fix.
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
- Dev/publish remote: `origin` (`microsoft/intelligent-terminal`); branches are
  local only. No push, PR, or product commit is requested in this phase.
- Related product PR: `microsoft/intelligent-terminal#1155`, merged as
  `ccdf84589cfc4d2d63af750c4050ea94565f767f` on 2026-10-16.
- External issue: `google-antigravity/antigravity-cli#921`.
- Evidence root: `artifacts\antigravity-acp-init-storm-20261019` in the dev
  worktree. This directory is ignored.
- Current publishable commit: none; implementation is explicitly out of scope.
- Current publish head: `a73ca6b8aa76acec1568465a445f59e8409725f8`.
- Original dirty worktree: `C:\ado\intelligent-terminal`, left untouched.

## Current Stage

2026-10-19: investigation complete; paused for discussion before implementation.
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

No product fix or regression test has been written. After the requested
reproduction succeeded, the optional full C++ package build was stopped through
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
- Stop after reproduction and evidence handoff. Do not implement a fix, add a
  product regression test, or start a review/fix loop in this phase.

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
| Focused GREEN/full suites | Not applicable: no implementation authorized | Not run |
| Exact publish fix validation | Not applicable: publish remains baseline | Not run |

## Implementation Record

No product implementation, tests, registry entries, provider settings, enterprise
policy, or authentication changes are authorized or made. Local-only diagnostic
orchestration and this investigation record are separate from the publish tree.

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
- Three attempts is a candidate limit, not an approved implementation decision.
  Distinguish three total attempts from an initial attempt plus three retries.
- Implementation remains explicitly paused. No behavior-changing edit is
  authorized by this investigation record.

Before implementation, agree on the attempt-count definition, retry-budget
scope/reset conditions, and explicit manual-retry behavior. Those decisions must
cover background discovery, foreground/helper startup, and other relevant ACP
startup entry points without killing unrelated sessions or shared providers.

## Evidence Limits

- This is an exact-main binary reproduction through the real automatic master
  discovery and process-launch path, not a mocked agent or injected state.
- This is not an end-to-end launch of a newly built GUI package. The optional
  package build was deliberately stopped after the mechanism was reproduced.
- The isolated installer command exercises the same production preparation
  code, but the bounded master run reused that freshly installed managed pair.
- No assertion is made that the reporter had exactly 417 ACP processes.
- No corrective behavior or proposed fix has been validated or implemented.

## Review Triage

No review requested. No remediation decision made. Findings must be discussed
after reproduction before changing product behavior.

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

## Completion Checklist For This Phase

- [x] Fetch latest main and record exact source.
- [x] Create independent dev/publish branches without changing the dirty worktree.
- [x] Fill the template with the investigation-only contract.
- [x] Trace installation, initialization, retries and process ownership.
- [x] Reproduce the actual enterprise failure with bounded process creation.
- [x] Record native dialog/error and process-lifetime evidence.
- [x] Verify runtime cleanup and preservation of unrelated state.
- [x] Record confirmed findings separately from hypotheses.
- [x] Stop for discussion without implementation.
