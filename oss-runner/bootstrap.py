from __future__ import annotations

import base64
import hashlib
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

CONTROL_REPO = "PNHD/project-orchestrator"
CONTROL_OWNER = "PNHD"
CONTROL_OWNER_ID = 26757735
CORE_COMMIT = "d2e4c2460baa82ee0a40c3bab31cdc91b75a84c6"
RUNNER_SHA256 = "815c7d25670eeb45ae95dbb25496c243c2b240be6677868cc427d16bf22c5627"
POLICY_SHA256 = "794e9672ea2253ca247735cb7f31d4a6d57d713c6b8fbdb0b2f5373ec1d58ff8"
SMOKE_ISSUE = 4
TASK_NAME = "PNHD OSS Runner"


def run(cmd: list[str], *, stdin: str | None = None, timeout: int = 60, check: bool = True) -> subprocess.CompletedProcess[str]:
    p = subprocess.run(cmd, input=stdin, text=True, encoding="utf-8", errors="replace",
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout, check=False)
    if check and p.returncode:
        raise RuntimeError(f"command failed ({p.returncode}): {' '.join(cmd)}\n{p.stdout}")
    return p


def find_gh() -> str:
    found = shutil.which("gh") or shutil.which("gh.exe")
    if found:
        return found
    candidate = Path.home() / "AppData/Local/Microsoft/WinGet/Packages/GitHub.cli_Microsoft.Winget.Source_8wekyb3d8bbwe/bin/gh.exe"
    if candidate.exists():
        return str(candidate)
    raise RuntimeError("GitHub CLI (gh) was not found")


def gh_json(gh: str, endpoint: str) -> object:
    return json.loads(run([gh, "api", endpoint]).stdout)


def fetch_file(gh: str, path: str, expected_sha: str) -> bytes:
    data = gh_json(gh, f"repos/{CONTROL_REPO}/contents/{path}?ref={CORE_COMMIT}")
    if not isinstance(data, dict) or data.get("encoding") != "base64" or data.get("type") != "file":
        raise RuntimeError(f"unexpected GitHub response for {path}")
    raw = base64.b64decode(data.get("content", ""), validate=False)
    actual = hashlib.sha256(raw).hexdigest()
    if actual != expected_sha:
        raise RuntimeError(f"SHA-256 mismatch for {path}: expected {expected_sha}, got {actual}")
    return raw


def atomic_write(path: Path, raw: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_bytes(raw)
    os.replace(tmp, path)


def quote_cmd(path: Path | str) -> str:
    return '"' + str(path).replace('"', '""') + '"'


def main() -> int:
    print("=" * 74)
    print("PNHD OSS RUNNER BOOTSTRAP v1")
    print("=" * 74)
    print("Installs one guarded per-minute Windows executor. No merge/release/deploy rights.")
    if os.name != "nt":
        raise RuntimeError("This bootstrap is Windows-only")

    gh = find_gh()
    user = gh_json(gh, "user")
    if not isinstance(user, dict) or user.get("login") != CONTROL_OWNER or user.get("id") != CONTROL_OWNER_ID:
        raise RuntimeError("gh must be authenticated as PNHD/26757735")
    print("[OK] GitHub identity: PNHD/26757735")

    root = Path(os.environ.get("LOCALAPPDATA", str(Path.home() / "AppData/Local"))) / "PNHD/oss-runner"
    root.mkdir(parents=True, exist_ok=True)
    (root / "PAUSE").unlink(missing_ok=True)

    policy = fetch_file(gh, "oss-runner/policy.py", POLICY_SHA256)
    runner = fetch_file(gh, "oss-runner/runner.py", RUNNER_SHA256)
    atomic_write(root / "policy.py", policy)
    atomic_write(root / "runner.py", runner)
    if hashlib.sha256((root / "policy.py").read_bytes()).hexdigest() != POLICY_SHA256:
        raise RuntimeError("local policy verification failed")
    if hashlib.sha256((root / "runner.py").read_bytes()).hexdigest() != RUNNER_SHA256:
        raise RuntimeError("local runner verification failed")
    print(f"[OK] immutable runner installed from {CORE_COMMIT[:12]}")

    python = Path(sys.executable).resolve()
    run_cmd = root / "run.cmd"
    run_cmd.write_text(
        "@echo off\n"
        "set PYTHONUTF8=1\n"
        "set PYTHONIOENCODING=utf-8\n"
        f"{quote_cmd(python)} {quote_cmd(root / 'runner.py')} >> {quote_cmd(root / 'scheduled.log')} 2>&1\n",
        encoding="utf-8",
    )
    (root / "pause.cmd").write_text(f'@echo off\ntype nul > {quote_cmd(root / "PAUSE")}\necho PNHD OSS Runner paused.\n', encoding="utf-8")
    (root / "resume.cmd").write_text(f'@echo off\ndel /q {quote_cmd(root / "PAUSE")} 2>nul\necho PNHD OSS Runner resumed.\n', encoding="utf-8")
    (root / "uninstall.cmd").write_text(
        f'@echo off\nschtasks /Delete /TN "{TASK_NAME}" /F\necho Scheduled task removed. Runner files/logs were kept at {root}.\n',
        encoding="utf-8",
    )

    action = f'cmd.exe /d /c ""{run_cmd}""'
    create = run([
        "schtasks", "/Create", "/F", "/SC", "MINUTE", "/MO", "1",
        "/TN", TASK_NAME, "/TR", action, "/RL", "LIMITED", "/IT",
    ], timeout=60, check=False)
    if create.returncode:
        raise RuntimeError(
            "Scheduled Task creation failed. Open an Administrator CMD and run the same bootstrap command once.\n"
            + create.stdout
        )
    query = run(["schtasks", "/Query", "/TN", TASK_NAME, "/FO", "LIST", "/V"], timeout=60)
    if TASK_NAME.lower() not in query.stdout.lower():
        raise RuntimeError("Scheduled Task verification failed")
    print("[OK] Scheduled Task installed: every 1 minute, LIMITED privileges, current logged-in user")

    # Run the exact installed runner synchronously once so smoke evidence does not
    # depend on Task Scheduler timing.
    smoke_before = gh_json(gh, f"repos/{CONTROL_REPO}/issues/{SMOKE_ISSUE}")
    if not isinstance(smoke_before, dict):
        raise RuntimeError("smoke issue lookup failed")
    if smoke_before.get("state") == "open":
        print(f"[SMOKE] executing control issue #{SMOKE_ISSUE}")
        smoke = run([str(python), str(root / "runner.py")], timeout=180, check=False)
        if smoke.returncode != 0:
            raise RuntimeError(f"runner smoke execution failed ({smoke.returncode})\n{smoke.stdout}")

    deadline = time.time() + 30
    while time.time() < deadline:
        issue = gh_json(gh, f"repos/{CONTROL_REPO}/issues/{SMOKE_ISSUE}")
        comments = gh_json(gh, f"repos/{CONTROL_REPO}/issues/{SMOKE_ISSUE}/comments?per_page=100")
        bodies = [str(c.get("body", "")) for c in comments] if isinstance(comments, list) else []
        if isinstance(issue, dict) and issue.get("state") == "closed" and any(
            "<!-- PNHD_OSS_RESULT_V1 -->" in b and "PNHD OSS Runner: PASS" in b for b in bodies
        ):
            print("[OK] smoke-v1 closed with verified PASS result")
            break
        time.sleep(2)
    else:
        raise RuntimeError("smoke issue did not reach verified PASS")

    print("=" * 74)
    print("PASS: PNHD OSS RUNNER INSTALLED")
    print(f"root: {root}")
    print(f"core commit: {CORE_COMMIT}")
    print(f"pause: {root / 'pause.cmd'}")
    print(f"resume: {root / 'resume.cmd'}")
    print(f"uninstall task: {root / 'uninstall.cmd'}")
    print("Future guarded OSS jobs can now run without downloading per-job scripts.")
    print("=" * 74)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except Exception as exc:
        if os.name == "nt":
            try:
                subprocess.run(["schtasks", "/Delete", "/TN", TASK_NAME, "/F"],
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=30, check=False)
            except Exception:
                pass
            try:
                fail_root = Path(os.environ.get("LOCALAPPDATA", str(Path.home() / "AppData/Local"))) / "PNHD/oss-runner"
                fail_root.mkdir(parents=True, exist_ok=True)
                (fail_root / "PAUSE").write_text("bootstrap failed; runner paused\n", encoding="utf-8")
            except Exception:
                pass
        print("=" * 74)
        print("BOOTSTRAP BLOCKED - scheduled task removed and runner paused")
        print(str(exc))
        print("=" * 74)
        raise SystemExit(2)
