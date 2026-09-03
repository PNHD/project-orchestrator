# PNHD_OSS_JOB: validate-python-shell-333-v1
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

EXPECTED_PR = 333
EXPECTED_HEAD = "b4fd48c4803df3580f82cf89004b8627790de32c"
EXPECTED_BASE = "1cf1bfcac6ccde265bf0ac0e422fc54379941ed0"
UPSTREAM = "extrabacon/python-shell"
FORK_URL = "https://github.com/PNHD/python-shell.git"


def run(cmd: list[str], cwd: Path | None = None, timeout: int = 900) -> str:
    print("[JOB] $ " + " ".join(cmd), flush=True)
    proc = subprocess.run(
        cmd,
        cwd=cwd,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
        check=False,
    )
    print(proc.stdout, end="", flush=True)
    if proc.returncode != 0:
        raise RuntimeError(f"command failed with exit {proc.returncode}: {' '.join(cmd)}")
    return proc.stdout


def main() -> int:
    if os.environ.get("PNHD_OSS_RUNNER") != "1":
        raise RuntimeError("not running under PNHD OSS Runner")

    perms = set(filter(None, os.environ.get("PNHD_OSS_PERMISSIONS", "").split(",")))
    if perms != {"github_read", "local_build_test"}:
        raise RuntimeError(f"unexpected permission set: {sorted(perms)}")

    gh = shutil.which("gh") or shutil.which("gh.exe")
    git = shutil.which("git") or shutil.which("git.exe")
    npm = shutil.which("npm") or shutil.which("npm.cmd")
    npx = shutil.which("npx") or shutil.which("npx.cmd")
    if not all((gh, git, npm, npx)):
        missing = [name for name, value in (("gh", gh), ("git", git), ("npm", npm), ("npx", npx)) if not value]
        raise RuntimeError("missing required tools: " + ", ".join(missing))

    pr_raw = run([gh, "api", f"repos/{UPSTREAM}/pulls/{EXPECTED_PR}"], timeout=60)
    pr = json.loads(pr_raw)
    head = ((pr.get("head") or {}).get("sha"))
    base = ((pr.get("base") or {}).get("sha"))
    if pr.get("state") != "open" or pr.get("merged") is True:
        print(f"PNHD_RESULT: BLOCKED python-shell#{EXPECTED_PR} is no longer an open unmerged PR")
        return 3
    if head != EXPECTED_HEAD or base != EXPECTED_BASE:
        print(f"PNHD_RESULT: BLOCKED stale validation target; head={head} base={base}")
        return 3

    work = Path.cwd() / "python-shell"
    if work.exists():
        raise RuntimeError(f"unexpected pre-existing work directory: {work}")

    run([git, "clone", "--no-checkout", FORK_URL, str(work)], timeout=300)
    run([git, "-C", str(work), "checkout", "--detach", EXPECTED_HEAD], timeout=120)
    actual_head = run([git, "-C", str(work), "rev-parse", "HEAD"], timeout=30).strip()
    if actual_head != EXPECTED_HEAD:
        raise RuntimeError(f"checked out wrong HEAD: {actual_head}")

    run([npm, "ci"], cwd=work, timeout=900)
    test_out = run([npm, "test"], cwd=work, timeout=900)
    run([npx, "prettier", "--check", "index.ts", "test/test-python-shell.ts"], cwd=work, timeout=300)
    run([git, "diff", "--check"], cwd=work, timeout=60)

    passing_line = next((line.strip() for line in reversed(test_out.splitlines()) if "passing" in line), "npm test passed")
    print(f"PNHD_RESULT: PASS python-shell#{EXPECTED_PR} current head {EXPECTED_HEAD[:12]} validated")
    print(f"PNHD_RESULT: {passing_line}; npm ci; prettier check; git diff --check all passed")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        print(f"PNHD_RESULT: FAIL python-shell#{EXPECTED_PR}: {exc}")
        raise
