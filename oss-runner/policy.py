from __future__ import annotations

import hashlib
import json
import re
from typing import Any

CONTROL_REPO = "PNHD/project-orchestrator"
CONTROL_OWNER = "PNHD"
CONTROL_OWNER_ID = 26757735
TITLE_PREFIX = "[OSS-RUNNER]"
START_MARKER = "<!-- PNHD_OSS_JOB_V1 -->"
END_MARKER = "<!-- /PNHD_OSS_JOB_V1 -->"
RESULT_MARKER = "<!-- PNHD_OSS_RESULT_V1 -->"
CLAIM_MARKER = "<!-- PNHD_OSS_CLAIM_V1 -->"
MAX_TIMEOUT = 7200
MAX_ISSUE_BODY_CHARS = 32000
MAX_PAYLOAD_BYTES = 512000
PUBLIC_RESULT_PREFIX = "PNHD_RESULT:"

ALLOWED_PERMISSIONS = {
    "github_read",
    "github_write_review",
    "github_write_comment",
    "github_write_pr_metadata",
    "github_create_pr",
    "git_push_fork",
    "git_create_fork_branch",
    "local_build_test",
}

# Defense in depth only. Exact owner + immutable ref + SHA-256 are the trust root.
FORBIDDEN = [
    r"\bgit\s+push\b[^\n\r]*--force(?:\b|-with-lease)",
    r"\bgit\s+push\s+-f\b",
    r"\bgit\s+reset\s+--hard\b",
    r"\bgit\s+clean\s+-[^\n\r]*f",
    r"\bgh\s+pr\s+merge\b",
    r"\bgh\s+release\b",
    r"\bgh\s+repo\s+delete\b",
    r"pulls/[^\s\"']+/merge",
    r"\bwrangler\s+deploy\b",
    r"\bsupabase\s+(?:db|migration)\s+(?:push|up)\b",
    r"\bterraform\s+apply\b",
    r"\bkubectl\s+(?:apply|delete)\b",
    r"\bdocker\s+push\b",
    r"\baws\s+cloudformation\s+deploy\b",
    r"\bgcloud\s+[^\n\r]*deploy\b",
    r"\baz\s+deployment\b",
    r"\bopenclaw\s+doctor\s+--fix\b",
]

PERMISSION_PATTERNS = {
    "git_push_fork": [r"\bgit\s+push\b"],
    "git_create_fork_branch": [r"\bgit\s+(?:checkout\s+-b|switch\s+-c|branch\s+)"],
    "github_write_review": [r"/pulls/[^\s\"']+/reviews\b", r"\bgh\s+pr\s+review\b"],
    "github_write_comment": [r"/issues/[^\s\"']+/comments\b", r"\bgh\s+(?:pr|issue)\s+comment\b"],
    "github_write_pr_metadata": [r"\bgh\s+pr\s+edit\b", r"--method\s+PATCH[^\n\r]*/pulls/"],
    "github_create_pr": [r"\bgh\s+pr\s+create\b", r"--method\s+POST[^\n\r]*/pulls\b"],
}

SECRET_PATTERNS = [
    (re.compile(r"\bgh[pousr]_[A-Za-z0-9_]{20,}\b"), "<REDACTED_GITHUB_TOKEN>"),
    (re.compile(r"\bgithub_pat_[A-Za-z0-9_]{20,}\b"), "<REDACTED_GITHUB_TOKEN>"),
    (re.compile(r"\bsk-(?:proj-)?[A-Za-z0-9_-]{20,}\b"), "<REDACTED_API_KEY>"),
    (re.compile(r"(?im)^(authorization\s*:\s*).+$"), r"\1<REDACTED>"),
    (re.compile(r"(?im)^(cookie\s*:\s*).+$"), r"\1<REDACTED>"),
]


def parse_job(body: str) -> dict[str, Any]:
    if len(body) > MAX_ISSUE_BODY_CHARS:
        raise ValueError("issue body exceeds runner size limit")
    if START_MARKER not in body or END_MARKER not in body:
        raise ValueError("job markers are missing")
    raw = body.split(START_MARKER, 1)[1].split(END_MARKER, 1)[0].strip()
    job = json.loads(raw)
    if not isinstance(job, dict) or job.get("schema") != 1:
        raise ValueError("unsupported job schema")
    job_id = job.get("job_id")
    if not isinstance(job_id, str) or not re.fullmatch(r"[A-Za-z0-9._-]{3,96}", job_id):
        raise ValueError("invalid job_id")
    if job.get("kind") != "python_payload":
        raise ValueError("only python_payload is supported")
    timeout = job.get("timeout_seconds", 900)
    if not isinstance(timeout, int) or not 10 <= timeout <= MAX_TIMEOUT:
        raise ValueError("invalid timeout_seconds")
    perms = job.get("permissions", [])
    if not isinstance(perms, list) or any(p not in ALLOWED_PERMISSIONS for p in perms):
        raise ValueError("unsupported permission")
    payload = job.get("payload")
    if not isinstance(payload, dict) or payload.get("repo") != CONTROL_REPO:
        raise ValueError("payload must be in control repo")
    path = payload.get("path")
    expected_path = f"oss-runner/jobs/{job_id}.py"
    if path != expected_path:
        raise ValueError(f"payload path must be exactly {expected_path}")
    ref = payload.get("ref")
    digest = payload.get("sha256")
    if not isinstance(ref, str) or not re.fullmatch(r"[0-9a-f]{40}", ref):
        raise ValueError("payload ref must be exact commit SHA")
    if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise ValueError("invalid payload sha256")
    return job


def verify_payload(raw: bytes, job: dict[str, Any]) -> str:
    if len(raw) > MAX_PAYLOAD_BYTES:
        raise ValueError("payload exceeds size limit")
    digest = hashlib.sha256(raw).hexdigest()
    if digest != job["payload"]["sha256"]:
        raise ValueError(f"payload SHA-256 mismatch: {digest}")
    source = raw.decode("utf-8")
    if "PNHD_OSS_JOB" not in source[:4000]:
        raise ValueError("payload missing PNHD_OSS_JOB header")
    for pattern in FORBIDDEN:
        if re.search(pattern, source, re.I | re.M):
            raise ValueError(f"forbidden operation: {pattern}")
    perms = set(job.get("permissions", []))
    for permission, patterns in PERMISSION_PATTERNS.items():
        if permission in perms:
            continue
        for pattern in patterns:
            if re.search(pattern, source, re.I | re.M):
                raise ValueError(f"undeclared permission {permission}: {pattern}")
    return source


def sanitize(text: str) -> str:
    text = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", text)
    text = re.sub(r"[\u200b-\u200f\u202a-\u202e\u2060-\u206f]", "", text)
    text = "".join(c for c in text if not (0xE0000 <= ord(c) <= 0xE007F))
    text = text.replace("```", "'''")
    for pattern, repl in SECRET_PATTERNS:
        text = pattern.sub(repl, text)
    return text[:6000]


def public_summary(output: str) -> str:
    lines = [line[len(PUBLIC_RESULT_PREFIX):].strip() for line in output.splitlines() if line.startswith(PUBLIC_RESULT_PREFIX)]
    if not lines:
        return "No PNHD_RESULT lines emitted; full output is local only."
    return sanitize("\n".join(lines[:40]))
