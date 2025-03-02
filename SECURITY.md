# Security Policy

## Supported Versions

Cyclonetix follows a versioning policy to ensure security updates are applied efficiently. The following versions are actively maintained:

| Version | Supported | Security Fixes |
|---------|-----------|----------------|
| Latest  | ✅        | ✅              |
| Previous Major Version | ✅ | ✅ |
| Older Versions | ❌ | ❌ |

Security patches will only be provided for actively supported versions. Users are encouraged to upgrade to the latest version to ensure security compliance.

## Reporting a Vulnerability

If you discover a security vulnerability in Cyclonetix, please report it responsibly:

1. **Email Security Contact**: Send a detailed report to **security@cyclonetix.com**.
2. **Provide Details**: Include a description of the issue, steps to reproduce, and any relevant logs or proof-of-concept code.
3. **Wait for Response**: We aim to acknowledge all reports within **48 hours** and will provide a status update within **7 days**.
4. **Coordinated Disclosure**: If a fix is required, we will work with you to coordinate a responsible disclosure timeline.

## Security Best Practices

To ensure secure deployment and usage of Cyclonetix, follow these best practices:

### 1. Authentication & Authorization
- Use **secure API keys or tokens** for authentication.
- Limit access to **trusted users and services** only.
- Enforce **role-based access control (RBAC)** where applicable.

### 2. Data Protection
- Store sensitive data using **strong encryption (AES-256, TLS 1.2/1.3)**.
- Avoid hardcoding secrets in configuration files—use **environment variables or secret management tools**.

### 3. Secure Deployment
- Deploy Cyclonetix on **hardened environments** (Kubernetes, cloud providers with security controls).
- Ensure **least privilege** for containers and orchestrators.
- Enable **logging and monitoring** to detect security incidents early.

### 4. Secure Communication
- Use **HTTPS/TLS** for all external and internal communication.
- Restrict access to exposed APIs with **firewall rules and IP whitelisting**.

### 5. Dependency Management
- Regularly update dependencies to mitigate **known vulnerabilities (CVEs)**.
- Use tools like **cargo-audit** to check for outdated dependencies in Rust.

### 6. Code Security
- Follow **secure coding guidelines** when contributing to Cyclonetix.
- Perform **code reviews** to catch security flaws before merging changes.

## Incident Response Policy

In case of a security breach:
1. **Immediate Response**: Contain the issue and assess the impact.
2. **Investigation**: Identify the root cause and gather relevant logs.
3. **Remediation**: Patch vulnerabilities, rotate affected credentials, and deploy fixes.
4. **Communication**: Notify affected users and stakeholders if necessary.
5. **Post-Mortem**: Conduct a review to prevent similar issues in the future.

## Responsible Disclosure Program
We value the contributions of security researchers. If you find a security issue, please report it responsibly. We recognize and appreciate contributions that help improve Cyclonetix security.

## Contact Information
For any security-related inquiries or to report vulnerabilities, contact:
- **Email**: security@cyclonetix.com

Thank you for helping keep Cyclonetix secure!

