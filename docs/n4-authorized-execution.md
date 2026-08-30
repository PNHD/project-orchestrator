# N4 Authorized Execution

## Verified local protocol

N4 uses the locally installed `codex-cli 0.149.1`. First-party local sources were `codex app-server --help` and the read-only `codex app-server generate-json-schema` output from that binary.

The installed v2 schema verifies `thread/start`, `turn/start`, `turn/started`, `turn/completed`, and `turn/interrupt`. `thread/start` supports `cwd`, `model`, `approvalPolicy`, and `sandbox`; `turn/start` requires `threadId` and text input and supports `cwd`, `model`, `effort`, `approvalPolicy`, and `sandboxPolicy`. `turn/interrupt` requires `threadId` and `turnId`. `turn/completed.params.turn.status` is the completion authority; the verified terminal values are `completed`, `interrupted`, and `failed`.

N4 sends only `read-only` / `readOnly` or `workspace-write` / `workspaceWrite` policy shapes. Both set `networkAccess: false`; workspace write supplies only the registered canonical project directory in `writableRoots`. It never sends the schema's `danger-full-access` / `dangerFullAccess` option.

## Run and security model

State version 2 adds append-only execution attempts with identifiers, task/project/worker linkage, selected model/effort, policy, provider IDs where available, timestamps, status, bounded provider-visible result, and sanitized error. Valid states are `QUEUED`, `STARTING`, `RUNNING`, `SUCCEEDED`, `FAILED`, `CANCELLED`, and `INTERRUPTED`. Retry always creates a new attempt.

APPROVED is still intent, not execution. Only `run_task`, called by the explicit Run control, creates a run. The backend revalidates approval, active project, canonical directory, and the one-active-run-per-task invariant. It does not accept frontend cwd.

Every application-owned state mutation uses a Tauri-managed mutex store boundary that encompasses read, mutation, validated temporary write, and replacement. Malformed existing state is reported and not overwritten. Active runs found at startup are moved once to `INTERRUPTED`; N4 never resumes or auto-starts them.

The execution adapter is separate from short-lived telemetry. It has bounded startup/request/cancel timeouts, request-ID correlation, Unicode newline JSON handling, bounded stderr draining, child cleanup, scoped turn interruption, bounded result storage, and sanitized errors. It never persists configuration, auth data, cookies, credentials, or raw provider dumps.

## Deferred

Provider approval dialogs, multi-worker routing, cloud sync, auto-resume, network-enabled execution, unrestricted execution, and N5 signing/distribution are out of scope.
