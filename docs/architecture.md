# Architecture

```text
React UI -> Tauri command -> ProviderManager boundary -> CodexProvider adapter -> codex app-server JSON-RPC
```

The UI consumes normalized `ProviderHealth`, `QuotaBucket`, `QuotaWindow`, `ModelCapability`, `ReasoningEffort`, and `ActivityEvent` objects. Codex wire shapes are confined to `src-tauri/src/lib.rs`; the frontend does not know JSON-RPC method names or response layouts.

## Implemented now

- Tauri 2 + React + TypeScript desktop shell.
- Read-only `initialize`, `account/rateLimits/read`, and `model/list` calls; the Codex boundary is limited to current metadata and telemetry.
- Request ID correlation, newline-delimited JSON handling, Unicode preservation, finite request timeouts, bounded stderr drain/discard, bounded child-process cleanup, and sanitized recoverable error states.
- Provider reset epochs are normalized at the boundary to `unix:<seconds>`; frontend timestamp parsing treats `unix:` strictly and recoverably. Live invoke rejection produces sanitized error health with empty telemetry, while model and reasoning metadata preserve dynamic provider values.
- Packaged frontend assets use the relative Vite base required by the Tauri bundle. The Windows-first icon sources are `src-tauri/icons/icon.svg` and `src-tauri/icons/icon.ico`; generated Android, iOS, macOS, and raster collateral is outside the N2-R1 source scope.
- Versioned local orchestration state in the normal app-data directory: project registry, task approval queue, execution-attempt history, activity timeline, onboarding completion, and one local worker health record. A Tauri-managed mutex serializes every read-mutate-write cycle; JSON is validated before a unique sibling temporary write.
- Responsive Overview, Projects, Workers/catalog, Tasks/Approvals, and Activity surfaces. Approval actions are local-only and do not execute a provider task.

## N4 execution boundary

- Only explicit Run creates a provider attempt for an APPROVED task; approval, refresh, reload, and startup never execute.
- Backend derives cwd from the registered canonical project path, permits one active run per task, and never accepts frontend cwd.
- A dedicated adapter uses verified app-server thread/turn operations, read-only by default or workspace-write constrained to the project root, with network disabled.
- Terminal attempts, cancellation, retry as a new attempt, bounded sanitized output, and startup interruption reconciliation are durable.

## N5 community release boundary

Version 0.3.0 adds first-run safety onboarding, a focused Settings release/safety surface, public documentation, Windows CI, community templates, and unsigned MSI/NSIS packaging. GitHub publication, tags, signing, remote creation, and hosted release distribution remain release-owner operations.

## Optional adapters

CCCC is an optional/reference adapter, not a core execution dependency. Claude, Discord, multi-account routing, mobile binaries, and automatic failover are not implemented.
