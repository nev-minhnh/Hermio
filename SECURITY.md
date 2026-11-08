# Security policy

Hermio handles mail credentials and tokens, so security reports are taken seriously.

## Reporting a vulnerability

Please **do not open a public issue**. Use GitHub's private vulnerability reporting
(*Security → Report a vulnerability* on https://github.com/nev-minhnh/Hermio) and include:

- the Hermio version and Windows version,
- steps to reproduce, and what an attacker could achieve.

You should get an answer within a week. Fixes are released as soon as possible and credited
unless you prefer otherwise.

## Scope notes

- Passwords, OAuth tokens and API keys are kept in Windows Credential Manager; configuration
  exports protect them with Windows DPAPI (current user).
- Remote content in mail is blocked by default (CSP); the AI assistant only reads mailboxes
  you allow and never sends mail on its own.
- Values embedded in an official build (a Google OAuth client for installed apps) are not
  secrets in the cryptographic sense and are not in scope.
