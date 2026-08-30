# Security Policy

## Supported version

Security fixes are considered for the latest community release. Version 0.3.0 is early access and is not a hosted service.

## Reporting a vulnerability

After the public repository exists, use GitHub's private vulnerability reporting or a private Security Advisory for the repository. Do not open a public issue containing exploit details, credentials, private paths, or user data.

Include the affected version, impact, minimal reproduction, and any suggested mitigation. Maintainers will acknowledge reports when community capacity allows; this project does not promise a formal response SLA or bug bounty.

## Security boundaries

Project Orchestrator stores orchestration state locally and runs tasks through a local Codex installation. READ ONLY is the default; WORKSPACE WRITE is explicit and limited to the registered project. Execution network access is disabled. Review [docs/threat-model.md](docs/threat-model.md) for trust boundaries and limitations.
