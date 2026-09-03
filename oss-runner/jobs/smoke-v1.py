# PNHD_OSS_JOB: smoke-v1
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys

if os.environ.get("PNHD_OSS_RUNNER") != "1":
    raise SystemExit("not running under PNHD OSS Runner")
if os.environ.get("PNHD_OSS_PERMISSIONS") != "github_read":
    raise SystemExit("unexpected permission set")

gh = shutil.which("gh") or shutil.which("gh.exe")
if not gh:
    raise SystemExit("gh not found in runner environment")

p = subprocess.run([gh, "api", "user"], text=True, encoding="utf-8", errors="replace",
                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
if p.returncode != 0:
    raise SystemExit(f"gh api user failed: {p.returncode}")
user = json.loads(p.stdout)
if user.get("login") != "PNHD" or user.get("id") != 26757735:
    raise SystemExit("GitHub identity mismatch")

v = subprocess.run([gh, "--version"], text=True, encoding="utf-8", errors="replace",
                   stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False, timeout=30)
gh_version = (v.stdout.splitlines() or ["unknown"])[0]
print(f"PNHD_RESULT: PASS smoke-v1; GitHub identity=PNHD/26757735")
print(f"PNHD_RESULT: Python={sys.version.split()[0]}; {gh_version}")
