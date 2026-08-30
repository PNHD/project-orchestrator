# Contributing

Thanks for helping improve Project Orchestrator. Keep changes focused, explain the user-visible reason, and preserve the explicit approval-and-Run safety model.

## Development setup

Use Windows with Node.js 24, npm 11, Rust 1.98 MSVC, Microsoft C++ build tools, WebView2, and a local Codex CLI.

```powershell
npm ci
npm test
npm run typecheck
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo check --manifest-path src-tauri/Cargo.toml
```

Run the native app with `npm run tauri dev`. Use disposable projects and isolated app data for execution testing.

## Pull requests

- Open an issue first for broad behavior, security-boundary, persistence-schema, or provider-protocol changes.
- Add deterministic tests for behavior changes.
- Do not add network-enabled execution, unrestricted writable roots, frontend-controlled cwd, auto-run, analytics, or credential/config writes.
- Keep user-facing claims aligned with verified behavior.
- Do not commit local state, evidence bundles, installers, tokens, private paths, or personal screenshots.

Report vulnerabilities through the process in [SECURITY.md](SECURITY.md), not a public issue.
