# Security Policy

ASH is security-sensitive because it is a shell and agent runtime. Please report vulnerabilities privately.

## Supported Versions

ASH is pre-1.0. Security fixes target the `main` branch and the latest published release.

## Reporting a Vulnerability

Use GitHub private vulnerability reporting if enabled for the repository. If that is unavailable, contact the maintainers privately and avoid public issue disclosure until a fix or mitigation is available.

Please include:

- Affected ASH version or commit.
- Operating system and terminal.
- Reproduction steps.
- Impact and any known workaround.

## Security Design Defaults

- Agent and plugin actions use `allow | ask | deny` permissions.
- `.env` reads are denied by default.
- Secrets should be referenced through environment variables, OS keychain helpers, or provider auth stores, not stored directly in `.ashrc`.
- Native command mode must not delegate to another shell.
