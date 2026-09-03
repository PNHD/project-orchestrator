# PNHD OSS Runner v2

A guarded Windows **declarative GitHub writer** for PNHD's ongoing public OSS maintenance workflow. It removes the repeated `download one .py -> run it -> paste output` loop for GitHub writes that ChatGPT's GitHub integration cannot perform because an upstream repository returns `403`.

## Why v2 replaced v1 before installation

The first design could execute arbitrary Python and local build/test commands under the same Windows account that holds PNHD's `gh` credentials. That would make dependency/build code part of the credential trust boundary. v1 smoke/build jobs were cancelled before first execution.

v2 does **not execute job code** and does **not run npm, pip, tests, compilers, shells, or repository scripts** from the queue. The installed runner contains the only executable logic. Queue items are immutable JSON manifests describing a small fixed set of GitHub operations.

## Supported operations

- smoke/identity verification;
- submit a PR review on an exact current head;
- add a comment to an open public issue/PR, optionally guarded by exact PR head;
- update title/body/`maintainer_can_modify` on an exact current PR head;
- create a PR from a PNHD-owned public fork branch after verifying its exact head and checking for duplicates.

It does not support merge, release, deployment, branch push, arbitrary command execution, workflow approval, secret/config mutation, DB migration, force-push, destructive Git, or private-repository targets.

## Queue trust model

1. Control repository is exactly `PNHD/project-orchestrator`.
2. Queue issue author must be login `PNHD` and immutable GitHub user ID `26757735`.
3. The issue only points to `oss-runner/jobs/<job_id>.json`.
4. The manifest is fetched from an exact 40-character commit SHA, never a moving branch.
5. Manifest bytes must match the SHA-256 embedded in the issue.
6. Manifest schema rejects unknown fields and unsupported action kinds.
7. GitHub PR writes are guarded by the exact current PR head SHA where applicable.
8. Target repositories must be public and active.
9. The local GitHub CLI identity must also be `PNHD` / `26757735`.

The repository is public. Never place credentials, private source, cookies, OAuth tokens, private-repository data, or secrets in queue issues/manifests.

## At-most-once behavior

The runner posts a trusted `PNHD_OSS_CLAIM_V2` marker immediately before fetching/applying the immutable manifest. A later invocation that sees a trusted claim without a result does **not replay the job**. It closes the queue item as blocked instead. This prefers manual recovery over duplicated external writes after an uncertain crash/network result.

## Local installation and controls

`bootstrap.py` installs immutable `policy.py` and `runner.py` bytes from a pinned commit after SHA-256 verification, compiles them, creates a Windows Scheduled Task named `PNHD OSS Runner`, then executes the declarative smoke issue synchronously.

The task runs once per minute with `LIMITED` privileges under the current Windows user and `/IT`, so it runs only while that user is logged on. It does not store or copy GitHub tokens; it uses the existing authenticated `gh` session.

Bootstrap also creates under `%LOCALAPPDATA%\PNHD\oss-runner`:

- `run.cmd` — fixed Scheduled Task entrypoint;
- `pause.cmd` — local kill switch;
- `resume.cmd` — resumes polling;
- `uninstall.cmd` — deletes the Scheduled Task while preserving local files/logs.

If bootstrap fails, it deletes the Scheduled Task and creates `PAUSE` so no unattended runner remains active.

## Builds/tests are a separate executor

Unattended build/test execution needs isolation from the credential-bearing Windows account. Do not reintroduce arbitrary Python or `local_build_test` into this runner. Use a separately verified sandboxed executor (for example an appropriately isolated Codex/OpenClaw/container/CI surface) before automating repository code execution.

## Manual gates that remain manual

Merge, release, production deployment, workflow/CI permission changes, credentials/secrets, DB migrations, force-push/history rewrite, destructive Git, and any expansion of the local runner's action surface require a separate Product Owner decision.
