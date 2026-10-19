# ACP provider lifecycle

## Problem and ownership

An installed agent CLI or ACP executable is not proof that ACP can initialize.
An OS loader failure can occur before the provider sends any protocol response.
For example, an enterprise Application Control policy can reject DLLs extracted
by Antigravity's standalone ACP distribution. The native error dialog can hold a
descendant alive after its launcher is terminated.

Intelligent Terminal must contain such failures regardless of provider or cause.
Neither an unknown failure nor a failed diagnostic is permission to retry
indefinitely, reinstall an already-present dependency, or leave child processes.

WTA owns the runtime lifecycle. Provider adapters describe provider differences;
the shared lifecycle owns admission, bounded execution, failure state and
process-tree cleanup. C++ Settings, onboarding, helper panes, history discovery
and model probes consume the same outcomes rather than defining independent
provider-specific recovery loops.

## Separate facts

| Fact | Meaning |
| --- | --- |
| Detected | The required executable and launch prerequisites were found. |
| Prepared | Required artifacts are present and preparation checks succeeded. |
| Startup checked | A bounded ACP initialize attempt completed or failed. |
| Protocol ready | ACP initialize succeeded for this live process. |
| Session ready | Required authentication and session creation/loading completed. |
| Selectable | Product policy allows selecting the provider; this is not a promise of successful startup. |

Detection is read-only. A previous successful check never replaces runtime
initialization. A previous failed check does not hide an installed provider or
prevent selecting it, unless product policy independently disallows it.
Failures of one session must not retire a healthy process shared by other
sessions.

## Shared mechanisms and provider adapters

Reuse the existing `AgentProfile` metadata and canonical provider identities.
The shape follows usage reporting: provider adapters contribute specific
behavior to a shared result model and shared UI projection.

Provider adapters own:

- CLI and ACP prerequisite detection.
- Supported preparation operations and their existing installation logic.
- Native ACP, npm-adapter, standalone-distribution and custom-command differences.
- Source-aware preparation requirements, using the existing registry and launch helpers.

Shared code owns:

- Policy and explicit-versus-automatic operation intent.
- Idempotent preparation and structured operation results.
- Startup admission, attempt limits, timeout handling and duplicate requests.
- Windows process jobs and WSL process groups, with exact ownership.
- Sanitized failure classification, status publication and retry actions.

Do not duplicate the registry, install credentials, alter enterprise enforcement,
or take ownership of user-supplied custom installations.

## Installation-time startup check

After an installation or repair actually prepares new ACP artifacts, perform one
bounded initialize check through the common lifecycle. This check must not send
a model prompt or silently initiate login.

Where installation is already followed by a real startup, that first startup
provides the verification result. Do not start a second disposable process just
to repeat the same check. A standalone preparation operation must own and clean
up any temporary process it uses.

Return preparation and startup-check outcomes separately. If preparation
succeeds but initialize fails, report **installed, startup check failed**, with a
safe diagnostic and the available next action. Do not report that the dependency
is missing, remove the agent from the picker, revert the user's selection, or
restart installation automatically.

One failed verification ends that verification operation. The user can still
select the provider and explicitly retry from the agent pane. Authentication
requirements remain distinct from an invalid installation or an OS loader
failure.

The user-side cost is a cold process startup, its memory/CPU/I/O and bounded
waiting time. Initialize does not request model inference, although individual
providers may perform network/configuration work. Reusing the first live
startup avoids a duplicate cold launch. Runtime retry limits remain necessary
even after a successful installation-time check.

## Failure and retry contract

- Allow at most three total automatic startup attempts, including the first.
- Scope admission to the effective provider, execution source and launch
  configuration. Automatic callers must not independently reset the budget.
- Ordinary refresh, tab activation and duplicate requests do not create a new
  retry cycle. Explicit user retry is a distinct intent.
- Confirm the failed attempt's process tree has exited before admitting its
  replacement. If cleanup fails or ownership is uncertain, stop automatic
  replacement and surface the failure.
- Authentication, cancellation, session-local protocol errors and provider
  process startup failures are distinct outcomes.
- Installation-time verification participates in the same lifecycle ownership
  and accounting; it must not create an unbounded independent retry loop.
- Missing detailed stderr or an exact OS cause does not weaken the limit or
  cleanup requirement.

Preserve structured phase, reason, available numeric error codes, attempt count,
safe summary and permitted next actions. Keep credentials, raw provider
configuration and unrelated terminal data out of public status payloads.
Settings uses fully localized primary error text and the standard InfoBar message
and action properties. Technical phase/reason codes, attempt counts and diagnostic
IDs stay in structured CLI output and logs rather than the primary announcement.

The package-private `acp-startup` ledger uses an exclusive per-target file lease
so master startup and independent model-probe processes serialize their attempts.
Targets include the authoritative agent identity, execution source, command and
a restart-stable digest of effective provider binding fields. Process-local pool
generations are not used as persistent admission identities. Ledger directories are scoped to the WTA executable identity,
so installing a different build does not inherit an obsolete startup failure.
An explicit retry does not override an unconfirmed cleanup failure.
It advances a persisted retry epoch; stopped discovery/history workers observe
that epoch without ordinary refresh resetting the budget. Results from an older
epoch cannot stop a newer retry cycle.

The provider's initialize deadline is separate from the helper's overall startup
deadline, which also includes bounded admission, preparation and cleanup waits.
Receipt acquisition and post-initialize retirement keep process ownership and
the admission lease until cleanup is confirmed or quarantined.

Windows ACP children inherit critical-error reporting instead of a native loader
dialog. This changes error presentation only: Application Control enforcement,
the provider's extraction directory and DLL verification remain unchanged.
WSL children retain their verified process-group ownership and stdin-EOF
guardian.

## Validation

Use deterministic process/protocol fixtures for CI, including a launcher that
leaves a descendant. Cover the attempt ceiling, shared admission, cleanup before
retry, healthy siblings, explicit retry, installation-versus-startup failure,
and selection after a failed check.

Use local-only real-provider acceptance for the enterprise DLL rejection.
Record exact source/package/binary identity and verify both process and native
dialog cleanup. No token-consuming provider test belongs in the published E2E
suite or CI.
