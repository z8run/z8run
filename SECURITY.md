# Security Policy

## Supported Versions

We release security fixes for the latest minor version on the `main` branch. Older versions are not actively patched unless a critical vulnerability warrants a backport.

| Version | Supported |
|---------|-----------|
| latest (main) | Yes |
| older releases | No |

---

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues, discussions, or pull requests.**

If you believe you have found a security vulnerability in z8run, please disclose it responsibly by emailing:

**security@z8run.org**

Include as much of the following information as possible to help us understand and reproduce the issue:

- **Type of vulnerability** (e.g., remote code execution, privilege escalation, authentication bypass, injection)
- **Component affected** (e.g., z8run-api, z8run-runtime, WASM sandbox, credential vault, WebSocket protocol)
- **z8run version** (output of `z8run info`)
- **Step-by-step reproduction instructions**
- **Proof-of-concept or exploit code** (if available)
- **Potential impact** and attack scenarios
- **Your suggested fix** (optional but appreciated)

---

## Response Timeline

We take security reports seriously and will respond promptly:

| Milestone | Target |
|-----------|--------|
| Initial acknowledgement | Within 48 hours |
| Triage and severity assessment | Within 5 business days |
| Fix developed and reviewed | Within 30 days (critical issues prioritized) |
| Public disclosure | Coordinated with the reporter |

---

## Coordinated Disclosure

We follow a **coordinated disclosure** process:

1. You report the vulnerability privately to **security@z8run.org**.
2. We acknowledge receipt and begin triage.
3. We develop and test a fix internally.
4. We release the fix and a security advisory simultaneously.
5. You receive credit in the advisory (unless you prefer to remain anonymous).

We ask reporters to keep the vulnerability confidential until we have released a fix. We aim to resolve critical issues within 30 days and will communicate openly with you throughout the process.

---

## Scope

The following are **in scope** for security reports:

- Remote code execution or privilege escalation in the z8run server or CLI
- Authentication or authorization bypasses (JWT, credential vault)
- WASM sandbox escapes or capability violations
- SQL injection or data exfiltration vulnerabilities
- WebSocket protocol vulnerabilities enabling unauthorized access or data manipulation
- Cryptographic weaknesses in the AES-256-GCM credential vault
- Significant information disclosure vulnerabilities

The following are generally **out of scope**:

- Vulnerabilities in third-party dependencies (report those to the upstream project; we will still apply patches promptly)
- Denial-of-service attacks that require physical access or authenticated admin access
- Social engineering or phishing attacks
- Issues in environments not following our documented security configuration

---

## Security Best Practices for Self-Hosted Deployments

If you are running z8run in production, we recommend:

- **Do not expose port 7700 publicly** without a reverse proxy and TLS termination (see `deploy/` for Nginx examples).
- **Rotate JWT secrets** regularly via the `Z8_JWT_SECRET` environment variable.
- **Restrict plugin capabilities** to only what your WASM plugins require.
- **Use PostgreSQL** in production rather than the default embedded SQLite.
- **Keep z8run up to date** by watching this repository for new releases.
- **Use strong, unique credentials** for the credential vault.

---

## Contact

- **Security issues:** security@z8run.org
- **General questions:** hello@z8run.org
- **GitHub Security Advisories:** [z8run/z8run/security/advisories](https://github.com/z8run/z8run/security/advisories)

Thank you for helping keep z8run and its users safe.
