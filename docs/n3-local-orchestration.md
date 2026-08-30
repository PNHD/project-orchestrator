# N3 local orchestration MVP

## Objective

Provide a useful, local-first control plane: a durable project registry, one truthful local Codex worker view, an explicit approval queue, and a durable activity timeline. N3 does not execute approved tasks.

## Capability and data model

The versioned `OrchestrationState` JSON document contains `projects`, `tasks`, `activity`, and one `worker` record. Projects hold an id, display name, validated local path, timestamps, and archived flag. Tasks hold an id, project id, title, instruction, timestamps, status (`DRAFT`, `PENDING_APPROVAL`, `APPROVED`, or `CANCELLED`), and optional approval timestamp. Activity is structured with an id, type, timestamp, and optional project/task identifiers.

## Storage and command boundary

State is stored as `orchestration-state.json` under Tauri's application-data directory. Rust validates input and state transitions, serializes deterministically, writes a temporary sibling file, then replaces the primary file. A malformed state file is reported as a recoverable error and is never overwritten by an automatic reset.

Tauri commands expose state reads plus project and task lifecycle actions. `refresh_telemetry` remains the only provider command path and calls only `initialize`, `account/rateLimits/read`, and `model/list`; it can record a local worker-health transition but cannot start a thread or turn.

## UI flows

Overview summarizes projects, pending/approved tasks, worker health, quota telemetry, and recent activity. Projects supports add, edit, and archive. Tasks supports create, draft edit, submit, approve, and cancel. Workers renders the one real local Codex worker and its read-only metadata. Activity renders the durable timeline.

## Testing and success criteria

Rust tests cover state round-tripping, malformed persistence, project path/duplicate behavior, task lifecycle validation, and activity ordering. Existing frontend tests plus typecheck/build cover the UI boundary. Final acceptance also requires a native workflow/restart, 390px responsive proof, MSI and NSIS debug bundles, explicit provider-mutation inspection, and a clean single N3 commit.

## Deferred

N4 owns provider task execution and run outcomes. N5 owns release/signing/distribution. Cloud sync, accounts, external integrations, and multi-worker routing remain out of scope.
