# Project Orchestrator v0.3.0 release contract

## Positioning and support

Project Orchestrator v0.3.0 is a Windows-first early-access community release under the MIT License. It is a local-first desktop application for one local Codex installation. It is not marketed as a production-grade multi-user, cloud, multi-account, or multi-provider service.

- Supported release target: Windows 10/11 with WebView2.
- Native release-candidate tested Codex app-server: `codex-cli 0.151.0-alpha.7.2`.
- Community support: best effort through repository issues and discussions; no response SLA or warranty.
- Signing: MSI and NSIS installers are unsigned unless artifact metadata explicitly proves an authorized signature.

## Implemented release capabilities

- Local project registry with canonical existing-directory validation.
- Draft → pending approval → approved/cancelled task lifecycle.
- Approval records intent only; only an explicit Run action creates an execution attempt.
- READ_ONLY execution by default.
- Explicit WORKSPACE_WRITE execution constrained to the registered project root.
- Execution network access disabled for both policies.
- Cancellation, terminal history, retry as a new attempt, and bounded provider-visible results.
- Versioned local persistence for projects, tasks, activity, runs, worker health, and onboarding completion.
- Startup reconciliation marks incomplete attempts interrupted; it does not auto-run or auto-resume.
- First-run safety onboarding and a focused Settings release/safety surface.

## Local data and trust

Application-owned orchestration state is stored in the OS application-data location displayed in Settings. Local project paths, task instructions, approval history, and bounded run results can be sensitive. Codex authentication/configuration remains outside the application's write surface. No analytics, cloud telemetry, or hosted error reporting is included.

## Known limitations

- Only the Windows release candidate is validated for v0.3.0.
- One local Codex worker; no Claude, hosted-provider, multi-account, or automatic failover support.
- No cloud sync, mobile client, remote control plane, or multi-user authorization model.
- Provider output is bounded and not a full execution transcript.
- The tested Codex app-server is a prerelease build; compatibility must be revalidated when the local Codex version changes.
- Unsigned installers can trigger Windows publisher warnings.
- Code signing and GitHub publication are release-owner gates outside local acceptance.

## Local release acceptance criteria

The local release is acceptable only when versions are synchronized at 0.3.0; lifecycle, migration, onboarding, security, frontend, Rust, responsive, and isolated native execution checks pass; npm has no unresolved high/critical audit finding; no plausible real secret is found; current sanitized screenshots and both unsigned installer formats are produced; the final staged candidate receives adversarial review; exactly one release-prep commit is created on the accepted N4 parent; and one self-consistent checksummed evidence ZIP is verified. Publication, tags, signing, remote creation, and GitHub Release creation are explicitly excluded.
