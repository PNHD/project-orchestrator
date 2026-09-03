from __future__ import annotations

import hashlib
import json
import re
from typing import Any

CONTROL_REPO = "PNHD/project-orchestrator"
CONTROL_OWNER = "PNHD"
CONTROL_OWNER_ID = 26757735
TITLE_PREFIX = "[OSS-RUNNER]"
START_MARKER = "<!-- PNHD_OSS_JOB_V2 -->"
END_MARKER = "<!-- /PNHD_OSS_JOB_V2 -->"
RESULT_MARKER = "<!-- PNHD_OSS_RESULT_V2 -->"
CLAIM_MARKER = "<!-- PNHD_OSS_CLAIM_V2 -->"
MAX_ISSUE_BODY_CHARS = 16000
MAX_MANIFEST_BYTES = 64000
MAX_TEXT = 12000

REPO_RE = re.compile(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\Z")
SHA_RE = re.compile(r"[0-9a-f]{40}\Z")
JOB_RE = re.compile(r"[A-Za-z0-9._-]{3,96}\Z")
BRANCH_RE = re.compile(r"[A-Za-z0-9._/-]{1,200}\Z")

SECRET_PATTERNS = [
    (re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b"), "<REDACTED_GITHUB_TOKEN>"),
    (re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"), "<REDACTED_GITHUB_TOKEN>"),
    (re.compile(r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b"), "<REDACTED_API_KEY>"),
]


def reject_unknown(obj: dict[str, Any], allowed: set[str], where: str) -> None:
    extra = set(obj) - allowed
    if extra:
        raise ValueError(f"unsupported fields in {where}: {sorted(extra)}")


def safe_text(value: Any, name: str, *, allow_empty: bool = False, max_len: int = MAX_TEXT) -> str:
    if not isinstance(value, str):
        raise ValueError(f"{name} must be a string")
    if not allow_empty and not value.strip():
        raise ValueError(f"{name} must not be empty")
    if len(value) > max_len:
        raise ValueError(f"{name} exceeds {max_len} characters")
    if "\x00" in value or re.search(r"[\u200b-\u200f\u202a-\u202e\u2060-\u206f]", value):
        raise ValueError(f"{name} contains forbidden invisible/bidi controls")
    if any(0xE0000 <= ord(c) <= 0xE007F for c in value):
        raise ValueError(f"{name} contains forbidden Unicode tag controls")
    return value


def validate_repo(value: Any, name: str = "repo") -> str:
    if not isinstance(value, str) or not REPO_RE.fullmatch(value):
        raise ValueError(f"invalid {name}")
    return value


def validate_sha(value: Any, name: str = "sha") -> str:
    if not isinstance(value, str) or not SHA_RE.fullmatch(value):
        raise ValueError(f"invalid {name}")
    return value


def validate_positive_int(value: Any, name: str) -> int:
    if not isinstance(value, int) or isinstance(value, bool) or value < 1:
        raise ValueError(f"invalid {name}")
    return value


def parse_pointer(body: str) -> dict[str, Any]:
    if len(body) > MAX_ISSUE_BODY_CHARS:
        raise ValueError("issue body exceeds size limit")
    if START_MARKER not in body or END_MARKER not in body:
        raise ValueError("v2 job markers are missing")
    raw = body.split(START_MARKER, 1)[1].split(END_MARKER, 1)[0].strip()
    pointer = json.loads(raw)
    if not isinstance(pointer, dict):
        raise ValueError("job pointer must be an object")
    reject_unknown(pointer, {"schema", "job_id", "manifest"}, "job pointer")
    if pointer.get("schema") != 2:
        raise ValueError("unsupported job pointer schema")
    job_id = pointer.get("job_id")
    if not isinstance(job_id, str) or not JOB_RE.fullmatch(job_id):
        raise ValueError("invalid job_id")
    manifest = pointer.get("manifest")
    if not isinstance(manifest, dict):
        raise ValueError("manifest pointer is required")
    reject_unknown(manifest, {"repo", "path", "ref", "sha256"}, "manifest pointer")
    if manifest.get("repo") != CONTROL_REPO:
        raise ValueError("manifest must live in control repo")
    if manifest.get("path") != f"oss-runner/jobs/{job_id}.json":
        raise ValueError("manifest path/job_id binding failed")
    validate_sha(manifest.get("ref"), "manifest ref")
    digest = manifest.get("sha256")
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise ValueError("invalid manifest sha256")
    return pointer


def parse_manifest(raw: bytes, pointer: dict[str, Any]) -> dict[str, Any]:
    if len(raw) > MAX_MANIFEST_BYTES:
        raise ValueError("manifest exceeds size limit")
    actual = hashlib.sha256(raw).hexdigest()
    if actual != pointer["manifest"]["sha256"]:
        raise ValueError(f"manifest SHA-256 mismatch: {actual}")
    manifest = json.loads(raw.decode("utf-8"))
    if not isinstance(manifest, dict):
        raise ValueError("manifest must be an object")
    if manifest.get("schema") != 2 or manifest.get("job_id") != pointer["job_id"]:
        raise ValueError("manifest schema/job_id binding failed")
    kind = manifest.get("kind")
    if kind == "smoke":
        reject_unknown(manifest, {"schema", "job_id", "kind"}, "smoke manifest")
    elif kind == "github_review":
        reject_unknown(manifest, {"schema", "job_id", "kind", "target", "review"}, "github_review manifest")
        _review(manifest)
    elif kind == "github_comment":
        reject_unknown(manifest, {"schema", "job_id", "kind", "target", "comment"}, "github_comment manifest")
        _comment(manifest)
    elif kind == "github_pr_update":
        reject_unknown(manifest, {"schema", "job_id", "kind", "target", "update"}, "github_pr_update manifest")
        _pr_update(manifest)
    elif kind == "github_pr_create":
        reject_unknown(manifest, {"schema", "job_id", "kind", "target", "pull_request"}, "github_pr_create manifest")
        _pr_create(manifest)
    else:
        raise ValueError(f"unsupported declarative job kind: {kind!r}")
    return manifest


def _pr_target(target: Any) -> dict[str, Any]:
    if not isinstance(target, dict):
        raise ValueError("target must be an object")
    reject_unknown(target, {"repo", "pr_number", "expected_head_sha"}, "PR target")
    validate_repo(target.get("repo"))
    validate_positive_int(target.get("pr_number"), "pr_number")
    validate_sha(target.get("expected_head_sha"), "expected_head_sha")
    return target


def _review(manifest: dict[str, Any]) -> None:
    _pr_target(manifest.get("target"))
    review = manifest.get("review")
    if not isinstance(review, dict):
        raise ValueError("review must be an object")
    reject_unknown(review, {"event", "body"}, "review")
    if review.get("event") not in {"APPROVE", "REQUEST_CHANGES", "COMMENT"}:
        raise ValueError("invalid review event")
    body = review.get("body", "")
    if review["event"] in {"REQUEST_CHANGES", "COMMENT"}:
        safe_text(body, "review body")
    else:
        safe_text(body, "review body", allow_empty=True)


def _comment(manifest: dict[str, Any]) -> None:
    target = manifest.get("target")
    if not isinstance(target, dict):
        raise ValueError("target must be an object")
    reject_unknown(target, {"repo", "issue_number", "expected_pr_head_sha"}, "comment target")
    validate_repo(target.get("repo"))
    validate_positive_int(target.get("issue_number"), "issue_number")
    if target.get("expected_pr_head_sha") is not None:
        validate_sha(target.get("expected_pr_head_sha"), "expected_pr_head_sha")
    comment = manifest.get("comment")
    if not isinstance(comment, dict):
        raise ValueError("comment must be an object")
    reject_unknown(comment, {"body"}, "comment")
    safe_text(comment.get("body"), "comment body")


def _pr_update(manifest: dict[str, Any]) -> None:
    _pr_target(manifest.get("target"))
    update = manifest.get("update")
    if not isinstance(update, dict):
        raise ValueError("update must be an object")
    reject_unknown(update, {"title", "body", "maintainer_can_modify"}, "PR update")
    if not update:
        raise ValueError("PR update must not be empty")
    if "title" in update:
        safe_text(update["title"], "PR title", max_len=256)
    if "body" in update:
        safe_text(update["body"], "PR body", allow_empty=True)
    if "maintainer_can_modify" in update and not isinstance(update["maintainer_can_modify"], bool):
        raise ValueError("maintainer_can_modify must be boolean")


def _pr_create(manifest: dict[str, Any]) -> None:
    target = manifest.get("target")
    if not isinstance(target, dict):
        raise ValueError("target must be an object")
    reject_unknown(target, {"repo", "base"}, "PR create target")
    validate_repo(target.get("repo"))
    base = target.get("base")
    if not isinstance(base, str) or not BRANCH_RE.fullmatch(base):
        raise ValueError("invalid base branch")
    pr = manifest.get("pull_request")
    if not isinstance(pr, dict):
        raise ValueError("pull_request must be an object")
    reject_unknown(pr, {"head_repo", "head_branch", "expected_head_sha", "title", "body", "maintainer_can_modify"}, "pull_request")
    head_repo = validate_repo(pr.get("head_repo"), "head_repo")
    if not head_repo.startswith(CONTROL_OWNER + "/"):
        raise ValueError("head_repo must be owned by PNHD")
    head_branch = pr.get("head_branch")
    if not isinstance(head_branch, str) or not BRANCH_RE.fullmatch(head_branch):
        raise ValueError("invalid head_branch")
    validate_sha(pr.get("expected_head_sha"), "expected_head_sha")
    safe_text(pr.get("title"), "PR title", max_len=256)
    safe_text(pr.get("body", ""), "PR body", allow_empty=True)
    if "maintainer_can_modify" in pr and not isinstance(pr["maintainer_can_modify"], bool):
        raise ValueError("maintainer_can_modify must be boolean")


def sanitize(text: str) -> str:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    text = re.sub(r"[\u200b-\u200f\u202a-\u202e\u2060-\u206f]", "", text)
    text = "".join(c for c in text if not (0xE0000 <= ord(c) <= 0xE007F))
    for pattern, repl in SECRET_PATTERNS:
        text = pattern.sub(repl, text)
    return text[:4000]
