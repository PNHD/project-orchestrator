# Changelog

All notable community-release changes are recorded here.

## [0.3.0] - 2026-08-30

### Added

- Windows-first Tauri desktop Mission Control with live local Codex telemetry and dynamic model/reasoning catalog (N2/N2-R1).
- Durable local project registry, task approval lifecycle, worker health, and activity timeline (N3).
- Explicit READ_ONLY and WORKSPACE_WRITE execution, cancellation, retry, attempt history, bounded provider output, and startup interruption reconciliation (N4).
- First-run safety onboarding, Settings release/safety surface, state-v3 migration, release contract, threat model, public documentation, community templates, Windows CI, and Dependabot configuration (N5).

### Changed

- Synchronized application, package, Tauri, Cargo, installer, and Codex client version declarations at 0.3.0.
- Execution activity now records `execution.started` once, when the provider turn is actually running.

### Security

- Execution cwd remains backend-derived from the registered project.
- READ_ONLY remains the default; WORKSPACE_WRITE remains constrained to that project.
- Network access remains disabled and no Codex config/auth write surface is exposed.

### Known limitations

- Windows-first early access with one local Codex worker.
- No cloud sync, multi-user service, multi-account routing, hosted provider, Claude adapter, or automatic failover.
- Installers are unsigned unless the release notes explicitly state otherwise.
