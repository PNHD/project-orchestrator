# PNHD OSS Runner v1

A guarded Windows executor for PNHD's ongoing OSS maintenance workflow. It replaces the repeated `download one .py -> run it -> paste output` loop with a persistent local runner that polls a PNHD-owned GitHub issue queue.

## Execution model

1. ChatGPT/PM audits an OSS task and writes a bounded payload to `oss-runner/jobs/<job_id>.py`.
2. The payload is committed to this repository and addressed by an exact 40-character commit SHA.
3. A PNHD-authored issue whose title begins `[OSS-RUNNER] <job_id>` contains the exact ref, SHA-256, timeout, and declared permissions.
4. The Windows runner polls once per minute, validates the job, claims it, executes it at most once, stores the full log locally, posts only explicit `PNHD_RESULT:` summary lines, and closes the issue.

## Trust boundary

This is intentionally a **trusted local executor, not a sandbox**. The primary trust root is all of the following together:

- control repository is exactly `PNHD/project-orchestrator`;
- issue author login is exactly `PNHD` and immutable GitHub user ID is `26757735`;
- only owner-authored claim/result comments are trusted;
- payload path is exactly `oss-runner/jobs/<job_id>.py`;
- payload is fetched from an immutable commit SHA, never from a moving branch;
- payload bytes must match the issue's SHA-256 before execution;
- the local `gh` identity must also be `PNHD` / `26757735`.

Because the control repository is public, untrusted users can read it and can comment on public issues. Their issue/comment markers are ignored. Do not put secrets, private source, credentials, cookies, OAuth tokens, or private-repository content into control issues or public payloads.

## At-most-once behavior

Before execution, the runner posts `PNHD_OSS_CLAIM_V1`. If a later invocation sees an existing trusted claim without a completed result, it **does not replay the job**; it blocks/closes it instead. This deliberately prefers manual recovery over accidentally repeating a GitHub write after a crash.

## Hard safety boundaries

The runner rejects common destructive or deployment actions, including force-push/history rewrite, PR merge, GitHub release/repository deletion, Wrangler deploy, Supabase DB/migration push, Terraform apply, kubectl apply/delete, Docker push, major cloud deploy commands, and `openclaw doctor --fix`.

Job permissions are declared explicitly (`github_read`, review/comment/PR metadata writes, PR creation, fork push/branch creation, and local build/test). Static checks are defense in depth; they are not an OS sandbox.

The runner itself does **not** authorize merge, release, production deploy, secret/config changes, DB migrations, force-push, or destructive Git operations. Those remain separate Product Owner gates.

## Output handling

Full stdout/stderr is retained only under `%LOCALAPPDATA%\PNHD\oss-runner\job-logs`.

Public GitHub result comments contain only lines deliberately prefixed by the payload with:

```text
PNHD_RESULT:
```

Before posting, the runner removes terminal escapes, invisible bidi/tag controls, Markdown fence injection, and common token/API-key patterns.

## Local controls

Bootstrap creates:

- `%LOCALAPPDATA%\PNHD\oss-runner\run.cmd`
- `pause.cmd` — creates a local `PAUSE` kill switch
- `resume.cmd` — removes the kill switch
- `uninstall.cmd` — removes the Scheduled Task but keeps local logs/files

The Scheduled Task is named `PNHD OSS Runner`, runs every minute with `LIMITED` privileges under the current Windows user, and uses the existing authenticated `gh` session.

## Recovery rule

Do not edit/reopen a claimed job to retry it. Create a new immutable payload commit and a new `[OSS-RUNNER]` issue with a new job ID. This preserves provenance and prevents accidental replay.
