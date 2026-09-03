from __future__ import annotations

import base64
import json
import os
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any
from urllib.parse import quote, urlencode

REVIEW_STATE = {"APPROVE": "APPROVED", "REQUEST_CHANGES": "CHANGES_REQUESTED", "COMMENT": "COMMENTED"}

from policy import (
    CLAIM_MARKER,
    CONTROL_OWNER,
    CONTROL_OWNER_ID,
    CONTROL_REPO,
    RESULT_MARKER,
    TITLE_PREFIX,
    parse_manifest,
    parse_pointer,
    sanitize,
)


def now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def root() -> Path:
    return Path(os.environ.get("LOCALAPPDATA", str(Path.home() / "AppData/Local"))) / "PNHD/oss-runner"


def log(message: str) -> None:
    root().mkdir(parents=True, exist_ok=True)
    line = f"[{now()}] {message}"
    print(line, flush=True)
    with (root() / "runner.log").open("a", encoding="utf-8") as fh:
        fh.write(line + "\n")


def find_gh() -> str:
    found = shutil.which("gh") or shutil.which("gh.exe")
    if found:
        return found
    candidate = Path.home() / "AppData/Local/Microsoft/WinGet/Packages/GitHub.cli_Microsoft.Winget.Source_8wekyb3d8bbwe/bin/gh.exe"
    if candidate.exists():
        return str(candidate)
    raise RuntimeError("GitHub CLI (gh) was not found")


def run(cmd: list[str], *, stdin: str | None = None, timeout: int = 60) -> str:
    proc = subprocess.run(
        cmd,
        input=stdin,
        text=True,
        encoding="utf-8",
        errors="replace",
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        timeout=timeout,
        check=False,
    )
    if proc.returncode:
        raise RuntimeError(f"command failed ({proc.returncode}): {' '.join(cmd)}\n{proc.stdout}")
    return proc.stdout


def gh_json(gh: str, endpoint: str, *, method: str = "GET", body: dict[str, Any] | None = None) -> Any:
    cmd = [gh, "api"]
    if method != "GET":
        cmd += ["--method", method]
    cmd.append(endpoint)
    if body is not None:
        cmd += ["--input", "-"]
        out = run(cmd, stdin=json.dumps(body, ensure_ascii=False))
    else:
        out = run(cmd)
    return json.loads(out) if out.strip() else None


def verify_identity(gh: str) -> None:
    user = gh_json(gh, "user")
    if not isinstance(user, dict) or user.get("login") != CONTROL_OWNER or user.get("id") != CONTROL_OWNER_ID:
        raise RuntimeError("gh must be authenticated as PNHD/26757735")


def verify_public_repo(gh: str, repo: str) -> dict[str, Any]:
    data = gh_json(gh, f"repos/{repo}")
    if not isinstance(data, dict) or data.get("private") is not False or data.get("archived") is True:
        raise RuntimeError(f"target repository must be public and active: {repo}")
    return data


def verify_pr(gh: str, target: dict[str, Any]) -> dict[str, Any]:
    repo = target["repo"]
    verify_public_repo(gh, repo)
    pr = gh_json(gh, f"repos/{repo}/pulls/{target['pr_number']}")
    if not isinstance(pr, dict) or pr.get("state") != "open" or pr.get("merged") is True:
        raise RuntimeError("target PR is no longer open/unmerged")
    head = (pr.get("head") or {}).get("sha")
    if head != target["expected_head_sha"]:
        raise RuntimeError(f"target PR head changed: expected {target['expected_head_sha']}, got {head}")
    return pr


def fetch_manifest(gh: str, pointer: dict[str, Any]) -> bytes:
    manifest = pointer["manifest"]
    path = quote(manifest["path"], safe="/")
    ref = quote(manifest["ref"], safe="")
    data = gh_json(gh, f"repos/{CONTROL_REPO}/contents/{path}?ref={ref}")
    if not isinstance(data, dict) or data.get("type") != "file" or data.get("encoding") != "base64":
        raise RuntimeError("manifest fetch did not return a base64 file")
    return base64.b64decode(data.get("content", ""), validate=False)


def apply_manifest(gh: str, manifest: dict[str, Any]) -> str:
    kind = manifest["kind"]
    if kind == "smoke":
        return "identity and declarative control-plane smoke passed"

    if kind == "github_review":
        target = manifest["target"]
        pr = verify_pr(gh, target)
        if (pr.get("user") or {}).get("login") == CONTROL_OWNER:
            raise RuntimeError("runner refuses to review PNHD's own PR")
        review = manifest["review"]
        expected_state = REVIEW_STATE[review["event"]]
        existing = gh_json(gh, f"repos/{target['repo']}/pulls/{target['pr_number']}/reviews?per_page=100")
        if isinstance(existing, list) and any(
            (item.get("user") or {}).get("login") == CONTROL_OWNER
            and (item.get("user") or {}).get("id") == CONTROL_OWNER_ID
            and item.get("commit_id") == target["expected_head_sha"]
            and item.get("state") == expected_state
            for item in existing
        ):
            return f"review {expected_state} already present on exact PR head {target['expected_head_sha'][:12]}"
        # Recheck immediately before the external write and anchor the review to
        # the immutable audited commit instead of GitHub's moving default head.
        verify_pr(gh, target)
        body: dict[str, Any] = {"event": review["event"], "commit_id": target["expected_head_sha"]}
        if review.get("body"):
            body["body"] = review["body"]
        result = gh_json(
            gh,
            f"repos/{target['repo']}/pulls/{target['pr_number']}/reviews",
            method="POST",
            body=body,
        )
        if not isinstance(result, dict) or result.get("state") != expected_state:
            raise RuntimeError(f"GitHub returned unexpected review state: {None if not isinstance(result, dict) else result.get('state')}")
        return f"review {expected_state} persisted on exact PR head {target['expected_head_sha'][:12]}"

    if kind == "github_comment":
        target = manifest["target"]
        verify_public_repo(gh, target["repo"])
        issue = gh_json(gh, f"repos/{target['repo']}/issues/{target['issue_number']}")
        if not isinstance(issue, dict) or issue.get("state") != "open":
            raise RuntimeError("comment target is not open")
        expected = target.get("expected_pr_head_sha")
        is_pr = "pull_request" in issue
        if is_pr and expected is None:
            raise RuntimeError("PR comments require expected_pr_head_sha")
        if expected is not None:
            pr = gh_json(gh, f"repos/{target['repo']}/pulls/{target['issue_number']}")
            if not isinstance(pr, dict) or (pr.get("head") or {}).get("sha") != expected:
                raise RuntimeError("comment target PR head changed")
        comment_body = manifest["comment"]["body"]
        existing = gh_json(gh, f"repos/{target['repo']}/issues/{target['issue_number']}/comments?per_page=100")
        if isinstance(existing, list):
            for item in existing:
                user = item.get("user") or {}
                if user.get("login") == CONTROL_OWNER and user.get("id") == CONTROL_OWNER_ID and item.get("body") == comment_body:
                    return f"identical PNHD comment already present as id {item.get('id')}"
        if is_pr:
            pr = gh_json(gh, f"repos/{target['repo']}/pulls/{target['issue_number']}")
            if not isinstance(pr, dict) or pr.get("state") != "open" or (pr.get("head") or {}).get("sha") != expected:
                raise RuntimeError("comment target PR changed immediately before write")
        result = gh_json(
            gh,
            f"repos/{target['repo']}/issues/{target['issue_number']}/comments",
            method="POST",
            body={"body": comment_body},
        )
        if not isinstance(result, dict) or not isinstance(result.get("id"), int):
            raise RuntimeError("GitHub did not return a persisted comment")
        return f"comment persisted as id {result['id']}"

    if kind == "github_pr_update":
        target = manifest["target"]
        pr = verify_pr(gh, target)
        author = pr.get("user") or {}
        if author.get("login") != CONTROL_OWNER or author.get("id") != CONTROL_OWNER_ID:
            raise RuntimeError("runner only updates PR metadata on PNHD-authored PRs")
        update = manifest["update"]
        if all(pr.get(key) == value for key, value in update.items()):
            return f"PR metadata already synchronized on exact head {target['expected_head_sha'][:12]}"
        verify_pr(gh, target)
        result = gh_json(
            gh,
            f"repos/{target['repo']}/pulls/{target['pr_number']}",
            method="PATCH",
            body=update,
        )
        if not isinstance(result, dict) or (result.get("head") or {}).get("sha") != target["expected_head_sha"]:
            raise RuntimeError("PR metadata update verification failed")
        for key, value in update.items():
            if result.get(key) != value:
                raise RuntimeError(f"PR metadata field did not persist: {key}")
        return f"PR metadata updated on exact head {target['expected_head_sha'][:12]}"

    if kind == "github_pr_create":
        target = manifest["target"]
        pr_data = manifest["pull_request"]
        verify_public_repo(gh, target["repo"])
        base_ref = gh_json(gh, f"repos/{target['repo']}/git/ref/heads/{quote(target['base'], safe='/')}")
        actual_base = ((base_ref or {}).get("object") or {}).get("sha") if isinstance(base_ref, dict) else None
        if actual_base != target["expected_base_sha"]:
            raise RuntimeError(f"base branch changed: expected {target['expected_base_sha']}, got {actual_base}")
        head_repo = verify_public_repo(gh, pr_data["head_repo"])
        if (head_repo.get("owner") or {}).get("login") != CONTROL_OWNER or (head_repo.get("owner") or {}).get("id") != CONTROL_OWNER_ID:
            raise RuntimeError("head repository is not owned by PNHD")
        ref = gh_json(gh, f"repos/{pr_data['head_repo']}/git/ref/heads/{quote(pr_data['head_branch'], safe='/')}")
        actual_head = ((ref or {}).get("object") or {}).get("sha") if isinstance(ref, dict) else None
        if actual_head != pr_data["expected_head_sha"]:
            raise RuntimeError(f"head branch changed: expected {pr_data['expected_head_sha']}, got {actual_head}")
        head_owner = pr_data["head_repo"].split("/", 1)[0]
        query = urlencode({"state": "open", "head": f"{head_owner}:{pr_data['head_branch']}", "base": target["base"], "per_page": 100})
        existing = gh_json(gh, f"repos/{target['repo']}/pulls?{query}")
        if isinstance(existing, list) and existing:
            current = existing[0]
            if (current.get("head") or {}).get("sha") == pr_data["expected_head_sha"]:
                return f"matching PR #{current.get('number')} already exists on exact head {pr_data['expected_head_sha'][:12]}"
            raise RuntimeError(f"matching open PR exists on a different head: #{current.get('number')}")
        # Recheck both immutable inputs immediately before PR creation.
        base_ref = gh_json(gh, f"repos/{target['repo']}/git/ref/heads/{quote(target['base'], safe='/')}")
        if ((base_ref or {}).get("object") or {}).get("sha") != target["expected_base_sha"]:
            raise RuntimeError("base branch changed immediately before PR creation")
        head_ref = gh_json(gh, f"repos/{pr_data['head_repo']}/git/ref/heads/{quote(pr_data['head_branch'], safe='/')}")
        if ((head_ref or {}).get("object") or {}).get("sha") != pr_data["expected_head_sha"]:
            raise RuntimeError("head branch changed immediately before PR creation")
        body = {
            "title": pr_data["title"],
            "head": f"{head_owner}:{pr_data['head_branch']}",
            "base": target["base"],
            "body": pr_data.get("body", ""),
            "maintainer_can_modify": pr_data.get("maintainer_can_modify", True),
        }
        result = gh_json(gh, f"repos/{target['repo']}/pulls", method="POST", body=body)
        if not isinstance(result, dict) or (result.get("head") or {}).get("sha") != pr_data["expected_head_sha"]:
            raise RuntimeError("created PR did not bind to expected head")
        return f"PR #{result.get('number')} created on exact head {pr_data['expected_head_sha'][:12]}"

    raise RuntimeError(f"unreachable manifest kind: {kind}")


def acquire_lock() -> Path | None:
    path = root() / "runner.lock"
    root().mkdir(parents=True, exist_ok=True)
    try:
        fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
        os.write(fd, f"pid={os.getpid()} started={now()}\n".encode("utf-8"))
        os.close(fd)
        return path
    except FileExistsError:
        if time.time() - path.stat().st_mtime > 600:
            path.unlink(missing_ok=True)
            return acquire_lock()
        return None


def issue_comments(gh: str, number: int) -> list[dict[str, Any]]:
    data = gh_json(gh, f"repos/{CONTROL_REPO}/issues/{number}/comments?per_page=100")
    return data if isinstance(data, list) else []


def trusted_comments(comments: list[dict[str, Any]]) -> list[dict[str, Any]]:
    return [
        c for c in comments
        if (c.get("user") or {}).get("login") == CONTROL_OWNER
        and (c.get("user") or {}).get("id") == CONTROL_OWNER_ID
    ]


def close_control_issue(gh: str, number: int, status: str, job_id: str, detail: str) -> None:
    detail = sanitize(detail)
    body = (
        f"{RESULT_MARKER}\n### PNHD OSS Runner v2: {status}\n\n"
        f"- job: `{job_id}`\n"
        f"- detail: {detail}\n"
        f"- completed: `{now()}`\n"
    )
    gh_json(gh, f"repos/{CONTROL_REPO}/issues/{number}/comments", method="POST", body={"body": body})
    gh_json(gh, f"repos/{CONTROL_REPO}/issues/{number}", method="PATCH", body={"state": "closed"})


def next_issue(gh: str) -> dict[str, Any] | None:
    data = gh_json(gh, f"repos/{CONTROL_REPO}/issues?state=open&per_page=100")
    if not isinstance(data, list):
        return None
    for issue in sorted(data, key=lambda item: int(item.get("number", 0))):
        user = issue.get("user") or {}
        if "pull_request" in issue:
            continue
        if user.get("login") != CONTROL_OWNER or user.get("id") != CONTROL_OWNER_ID:
            continue
        if str(issue.get("title", "")).startswith(TITLE_PREFIX):
            return issue
    return None


def process() -> int:
    if (root() / "PAUSE").exists():
        log("paused by local PAUSE file")
        return 0
    lock = acquire_lock()
    if lock is None:
        return 0
    try:
        gh = find_gh()
        verify_identity(gh)
        issue = next_issue(gh)
        if issue is None:
            log("no pending v2 job")
            return 0
        number = int(issue["number"])
        prior = trusted_comments(issue_comments(gh, number))
        if any(RESULT_MARKER in str(c.get("body", "")) for c in prior):
            gh_json(gh, f"repos/{CONTROL_REPO}/issues/{number}", method="PATCH", body={"state": "closed"})
            return 0
        if any(CLAIM_MARKER in str(c.get("body", "")) for c in prior):
            close_control_issue(gh, number, "BLOCKED", "unknown", "existing trusted claim found; job was not replayed")
            return 2
        pointer = parse_pointer(issue.get("body") or "")
        job_id = pointer["job_id"]
        title = str(issue.get("title", ""))
        bound = f"{TITLE_PREFIX} {job_id}"
        if title != bound and not title.startswith(bound + ":"):
            raise RuntimeError("issue title/job_id binding failed")
        fresh = gh_json(gh, f"repos/{CONTROL_REPO}/issues/{number}")
        if not isinstance(fresh, dict) or fresh.get("state") != "open" or fresh.get("body") != issue.get("body") or fresh.get("title") != issue.get("title"):
            raise RuntimeError("control issue changed during preflight")
        gh_json(
            gh,
            f"repos/{CONTROL_REPO}/issues/{number}/comments",
            method="POST",
            body={"body": f"{CLAIM_MARKER}\nRunner v2 claimed `{job_id}` at `{now()}`. Claimed jobs are never auto-replayed."},
        )
        raw = fetch_manifest(gh, pointer)
        manifest = parse_manifest(raw, pointer)
        detail = apply_manifest(gh, manifest)
        close_control_issue(gh, number, "PASS", job_id, detail)
        log(f"issue #{number} PASS: {job_id}")
        return 0
    except Exception as exc:
        log(f"BLOCKED: {exc}")
        try:
            if "number" in locals():
                close_control_issue(gh, number, "BLOCKED", locals().get("job_id", "unparsed"), str(exc))
        except Exception as report_exc:
            log(f"could not report BLOCKED result: {report_exc}")
        return 2
    finally:
        lock.unlink(missing_ok=True)


if __name__ == "__main__":
    if os.name != "nt":
        raise SystemExit("PNHD OSS Runner v2 is Windows-only")
    raise SystemExit(process())
