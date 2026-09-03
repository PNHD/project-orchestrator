from __future__ import annotations

import base64
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import quote

from policy import (
    CLAIM_MARKER, CONTROL_OWNER, CONTROL_OWNER_ID, CONTROL_REPO, RESULT_MARKER,
    TITLE_PREFIX, parse_job, public_summary, sanitize, verify_payload,
)


def now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def root() -> Path:
    return Path(os.environ.get("LOCALAPPDATA", str(Path.home() / "AppData/Local"))) / "PNHD/oss-runner"


def log(msg: str) -> None:
    root().mkdir(parents=True, exist_ok=True)
    line = f"[{now()}] {msg}"
    print(line, flush=True)
    with (root() / "runner.log").open("a", encoding="utf-8") as f:
        f.write(line + "\n")


def find_gh() -> str:
    found = shutil.which("gh") or shutil.which("gh.exe")
    if found:
        return found
    candidate = Path.home() / "AppData/Local/Microsoft/WinGet/Packages/GitHub.cli_Microsoft.Winget.Source_8wekyb3d8bbwe/bin/gh.exe"
    if candidate.exists():
        return str(candidate)
    raise RuntimeError("gh not found")


def run(cmd: list[str], *, stdin: str | None = None, timeout: int = 60) -> str:
    p = subprocess.run(cmd, input=stdin, text=True, encoding="utf-8", errors="replace",
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=timeout, check=False)
    if p.returncode:
        raise RuntimeError(f"command failed ({p.returncode}): {' '.join(cmd)}\n{p.stdout}")
    return p.stdout


def gh(gh_exe: str, endpoint: str, *, method: str = "GET", body: dict[str, Any] | None = None) -> Any:
    cmd = [gh_exe, "api"]
    if method != "GET":
        cmd += ["--method", method]
    cmd.append(endpoint)
    if body is not None:
        cmd += ["--input", "-"]
        out = run(cmd, stdin=json.dumps(body, ensure_ascii=False))
    else:
        out = run(cmd)
    return json.loads(out) if out.strip() else None


def verify_identity(gh_exe: str) -> None:
    user = gh(gh_exe, "user")
    if not isinstance(user, dict) or user.get("login") != CONTROL_OWNER or user.get("id") != CONTROL_OWNER_ID:
        raise RuntimeError("gh identity is not PNHD/26757735")


def acquire_lock() -> Path | None:
    p = root() / "runner.lock"
    root().mkdir(parents=True, exist_ok=True)
    try:
        fd = os.open(p, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        os.write(fd, f"pid={os.getpid()} {now()}\n".encode())
        os.close(fd)
        return p
    except FileExistsError:
        if time.time() - p.stat().st_mtime > 7800:
            p.unlink(missing_ok=True)
            return acquire_lock()
        return None


def comments(gh_exe: str, number: int) -> list[dict[str, Any]]:
    data = gh(gh_exe, f"repos/{CONTROL_REPO}/issues/{number}/comments?per_page=100")
    return data if isinstance(data, list) else []


def close_with(gh_exe: str, number: int, body: str) -> None:
    gh(gh_exe, f"repos/{CONTROL_REPO}/issues/{number}/comments", method="POST", body={"body": body})
    gh(gh_exe, f"repos/{CONTROL_REPO}/issues/{number}", method="PATCH", body={"state": "closed"})


def fetch_payload(gh_exe: str, job: dict[str, Any]) -> bytes:
    p = job["payload"]
    path = quote(p["path"], safe="/")
    ref = quote(p["ref"], safe="")
    data = gh(gh_exe, f"repos/{CONTROL_REPO}/contents/{path}?ref={ref}")
    if not isinstance(data, dict) or data.get("type") != "file" or data.get("encoding") != "base64":
        raise RuntimeError("payload fetch did not return a base64 file")
    return base64.b64decode(data.get("content", ""), validate=False)


def execute(raw: bytes, job: dict[str, Any], number: int) -> tuple[int, str, float]:
    verify_payload(raw, job)
    work = root() / "work" / str(number)
    work.mkdir(parents=True, exist_ok=True)
    payload = work / f"{job['job_id']}.py"
    payload.write_bytes(raw)
    env = os.environ.copy()
    env.update({
        "PYTHONUTF8": "1", "PYTHONIOENCODING": "utf-8", "PNHD_OSS_RUNNER": "1",
        "PNHD_OSS_JOB_ID": job["job_id"], "PNHD_OSS_ISSUE": str(number),
        "PNHD_OSS_PERMISSIONS": ",".join(job.get("permissions", [])),
    })
    started = time.time()
    p = subprocess.Popen([sys.executable, str(payload)], cwd=work, env=env, text=True,
                         encoding="utf-8", errors="replace", stdout=subprocess.PIPE,
                         stderr=subprocess.STDOUT,
                         creationflags=getattr(subprocess, "CREATE_NEW_PROCESS_GROUP", 0))
    try:
        out, _ = p.communicate(timeout=job.get("timeout_seconds", 900))
        return p.returncode, out or "", time.time() - started
    except subprocess.TimeoutExpired:
        subprocess.run(["taskkill", "/PID", str(p.pid), "/T", "/F"],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=15, check=False)
        try:
            out, _ = p.communicate(timeout=15)
        except subprocess.TimeoutExpired:
            p.kill(); out, _ = p.communicate()
        return 124, (out or "") + "\n[RUNNER] timeout; process tree terminated", time.time() - started


def next_issue(gh_exe: str) -> dict[str, Any] | None:
    data = gh(gh_exe, f"repos/{CONTROL_REPO}/issues?state=open&per_page=100")
    if not isinstance(data, list):
        return None
    for issue in sorted(data, key=lambda x: int(x.get("number", 0))):
        u = issue.get("user") or {}
        if "pull_request" in issue or u.get("login") != CONTROL_OWNER or u.get("id") != CONTROL_OWNER_ID:
            continue
        if issue.get("author_association") not in (None, "OWNER"):
            continue
        if str(issue.get("title", "")).startswith(TITLE_PREFIX):
            return issue
    return None


def process() -> int:
    if (root() / "PAUSE").exists():
        log("paused")
        return 0
    lock = acquire_lock()
    if lock is None:
        return 0
    try:
        gh_exe = find_gh(); verify_identity(gh_exe)
        issue = next_issue(gh_exe)
        if issue is None:
            log("no pending job")
            return 0
        number = int(issue["number"])
        prior = comments(gh_exe, number)
        if any(RESULT_MARKER in str(c.get("body", "")) for c in prior):
            gh(gh_exe, f"repos/{CONTROL_REPO}/issues/{number}", method="PATCH", body={"state": "closed"})
            return 0
        if any(CLAIM_MARKER in str(c.get("body", "")) for c in prior):
            close_with(gh_exe, number, f"{RESULT_MARKER}\n### PNHD OSS Runner: BLOCKED\n\nExisting claim found; job was not re-executed.")
            return 2
        job = parse_job(issue.get("body") or "")
        if not str(issue.get("title", "")).startswith(f"{TITLE_PREFIX} {job['job_id']}"):
            raise RuntimeError("title/job_id binding failed")
        gh(gh_exe, f"repos/{CONTROL_REPO}/issues/{number}/comments", method="POST",
           body={"body": f"{CLAIM_MARKER}\nRunner claimed `{job['job_id']}` at `{now()}`. A claimed job is never auto-replayed."})
        raw = fetch_payload(gh_exe, job)
        code, output, duration = execute(raw, job, number)
        logs = root() / "job-logs"; logs.mkdir(parents=True, exist_ok=True)
        local_log = logs / f"issue-{number}-{job['job_id']}.log"; local_log.write_text(output, encoding="utf-8", errors="replace")
        status = "PASS" if code == 0 else "FAIL"
        summary = public_summary(output)
        body = (f"{RESULT_MARKER}\n### PNHD OSS Runner: {status}\n\n"
                f"- job: `{job['job_id']}`\n- exit code: `{code}`\n- duration: `{duration:.1f}s`\n"
                f"- payload ref: `{job['payload']['ref']}`\n- payload sha256: `{job['payload']['sha256']}`\n"
                f"- full log: local only (`{local_log.name}`)\n\n```text\n{summary}\n```")
        close_with(gh_exe, number, body)
        log(f"issue #{number} {status}")
        return 0 if code == 0 else 1
    except Exception as exc:
        log(f"BLOCKED: {exc}")
        try:
            if 'number' in locals():
                close_with(gh_exe, number, f"{RESULT_MARKER}\n### PNHD OSS Runner: BLOCKED\n\n```text\n{sanitize(str(exc))}\n```")
        except Exception as report_exc:
            log(f"could not report BLOCKED: {report_exc}")
        return 2
    finally:
        lock.unlink(missing_ok=True)


if __name__ == "__main__":
    if os.name != "nt":
        raise SystemExit("Windows-only runner")
    raise SystemExit(process())
