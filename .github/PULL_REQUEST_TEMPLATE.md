## Summary

Describe the user-visible outcome and why it belongs in scope.

## Safety and privacy

- [ ] Approval still does not execute.
- [ ] READ ONLY remains the default.
- [ ] No network, unrestricted access, frontend cwd, config/auth writes, analytics, secrets, or private paths were added.

## Validation

- [ ] `npm test`
- [ ] `npm run typecheck`
- [ ] `npm run build`
- [ ] `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
- [ ] `cargo test --manifest-path src-tauri/Cargo.toml`
- [ ] `cargo check --manifest-path src-tauri/Cargo.toml`
