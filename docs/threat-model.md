# Threat model

## Scope and assets

Project Orchestrator is a local desktop control plane. Important assets are the user's registered workspaces, local orchestration state, task instructions, Codex authentication/configuration owned by Codex, approval decisions, provider-visible output, and any logs or evidence exported during validation.

## Trust boundaries

1. **User → React UI:** display names, local project paths, task titles/instructions, approval actions, execution policy, and model/reasoning selections are untrusted input.
2. **React UI → Tauri commands:** the backend validates state transitions. Execution cwd and writable roots are not accepted from the frontend.
3. **Application state → filesystem:** state is versioned and validated, with serialized read-mutate-write access and a sibling temporary file. Malformed state is reported and not automatically replaced.
4. **Backend → local Codex app-server:** telemetry and execution use newline-delimited JSON-RPC. Responses and provider-visible output are untrusted, correlated, sanitized, and bounded before persistence/display.
5. **Codex → workspace:** READ_ONLY is the default. WORKSPACE_WRITE is explicit and supplies only the registered canonical workspace as a writable root. Both disable execution network access.
6. **Validation/evidence boundary:** disposable workspaces and the absolute process-local `PROJECT_ORCHESTRATOR_DATA_DIR` override prevent release tests from using private projects or overwriting normal application state. The override is read only from the launching process environment, is never accepted from the frontend, and must resolve to an existing directory. Evidence must omit raw Codex config, credentials, cookies, tokens, and private workspace contents.

## Threats and controls

| Threat | Control | Residual risk |
| --- | --- | --- |
| Project path confusion or substitution | Canonicalize existing directories; reject duplicates; revalidate project, task approval, and path immediately before execution | A user can intentionally register a sensitive directory |
| Task executes on approval, startup, or reload | Only explicit `run_task` creates an attempt; startup only reconciles active attempts to interrupted | A user can still run a harmful instruction deliberately |
| Frontend broadens cwd or writable roots | Backend derives cwd from persisted canonical project; workspace-write supplies only that root | Provider/tool defects remain outside this app's complete control |
| Network or unrestricted execution | Fixed read-only/workspace-write policy construction with `networkAccess: false`; no danger/full-access option or setting | Local Codex implementation is an external dependency |
| Config/auth modification or secret persistence | No config/auth write RPC; state schema has no credential fields; errors are sanitized | Task text or provider output may contain secrets entered by the user |
| Raw or unbounded provider output | Request correlation, bounded stderr handling, sanitized errors, and bounded persisted result | Truncation can omit useful diagnostic context |
| Approval-response escalation | Approval handlers cannot broaden cwd, writable roots, or network; file changes are declined in READ_ONLY | Provider protocol changes require review |
| Corrupt or malicious local state | Strict version/schema validation; malformed state is preserved and reported | A local attacker with the user's filesystem rights can alter app data |
| UI injection | React text rendering; no `innerHTML` or `eval`; restrictive Tauri CSP | Third-party dependency vulnerabilities remain possible |
| Evidence/privacy leak | Sanitized demo state/screenshots; metadata-only config hashes; secret/history scan before commit | Git author identity is part of public Git history and needs owner review |

## Explicit non-goals

This release does not provide hostile multi-user isolation, cloud tenancy, remote access, credential management, arbitrary execution roots, network-enabled runs, or a guarantee that an intentionally approved workspace-write instruction is harmless.
