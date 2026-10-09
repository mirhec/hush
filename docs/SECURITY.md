# Security model and checks before production use

Hush has not undergone an external security audit. This document describes implemented controls and remaining limitations, not a security certification. Build and test results do not replace validation of native credential stores, notification delivery, and installers on each supported platform.

## Data flow

```text
System browser -> GitHub device authorization (github.com, HTTPS)
                          |
Hush login -> GitHub OAuth endpoints -> access and refresh tokens
                          |
                          v
                   OS credential store
                          |
                          v
GitHub API (api.github.com, HTTPS)
        |
        | Token in the Authorization header; read-only REST/GraphQL queries
        v
Hush background process -> private SQLite database -> egui window
        |
        +-> local operating system notification service

User clicks a notification link -> github.com in the system browser
```

There are no third-party API hosts or external image sources. The application code includes no telemetry, crash upload, updater, or push relay. The system browser handles GitHub sign-in and intentional link clicks according to its own configuration; operating systems may have their own diagnostic services. Statements about Hush's own code do not automatically cover every transitive dependency or the operating system.

## Credentials

- The default OAuth device flow requests `notifications`, `repo`, `read:org`, and `offline_access`. It uses a public client ID and no client secret. The user confirms the login directly on GitHub, without giving Hush a GitHub password.
- **The OAuth `repo` scope includes write access to private and public repositories.** GitHub does not provide the narrower general repository read scope needed to combine all of Hush's private-repository features into this single OAuth login. Hush itself does not issue repository mutations, but a stolen token could be used outside Hush with its full granted permissions.
- OAuth access and refresh tokens are stored in the OS credential store. Expiring access tokens are renewed with the refresh token. Revoked or expired credentials may require another browser login.
- The advanced manual-token mode retains a classic token limited to `notifications` and optionally `read:org`; additional classic scopes are rejected in that mode. Its optional fine-grained token should cover selected resources with only the required read permissions. The UI and connection checks verify identity, but not the fine-grained token's complete permission matrix.
- Credentials are stored through `keyring`: Linux Secret Service, macOS Keychain, and Windows Credential Manager. There is no fallback to JSON files, SQLite, environment variables, or `/tmp`.
- Tokens are never passed as subprocess arguments or transported through a shell or curl. Input buffers are cleared with `Zeroizing` after handoff where possible; the framework and HTTP library may still hold additional copies in memory. There is no protection guarantee against memory dumps, debuggers, or compromised processes running as the same user.
- `HeaderValue::set_sensitive(true)` suppresses normal debug output of the authentication header. HTTP errors are mapped to application-specific error types that contain no tokens.
- The credential store and SQLite cannot commit atomically together. If saving fails between the two operations, a credential store entry may remain. The error is not reported as a successful login.
- Disconnecting the account invalidates the database first, then removes its local credentials. Deletion failures are reported. To revoke access completely, also revoke the OAuth authorization or personal access tokens directly on GitHub.

## Network

Repository API targets must be exactly `https://api.github.com`, without user information, a different port, or a fragment. Redirects are rejected entirely, including those for renamed repositories; this can limit functionality but prevents credentials from being forwarded. Browser links must use exactly `https://github.com`.

Repository access uses only GET requests and two fixed, read-only GraphQL query structures through POST `/graphql`. OAuth login and renewal additionally use GitHub's fixed HTTPS device-code and access-token endpoints under `github.com`. There is no generic mutation or execution endpoint. Automatic proxy detection is disabled; corporate proxies are unsupported. TLS verification is never disabled. Response size, duration, page count, and request budget are bounded. IDs and repository paths are validated. Remote content is never executed as HTML, scripts, shell commands, or egui markup.

The app does not scan the contents of every accessible private repository. It can search the user's pull requests for reviews, however, and reads comments and reviews for discovered notification threads. Token permissions and configured rules determine the actual scope. Hush can therefore receive sensitive information even when banner contents remain private by default. Organization policies and SSO requirements can restrict access independently of a successful login.

## Local data and on-screen privacy

SQLite stores repository names, titles, actors, shortened event and comment contents, and technical thread tasks. **The database is not encrypted.** On Unix, the data directory is set to `0700` and the main file to `0600`. WAL and SHM files live in the private directory. Windows uses the local user profile and its ACLs, without an additional custom hardened ACL. There is no shared `/tmp` fallback.

The final directory component and data files are checked for symbolic links. This does not fully defend against privileged attackers, races in attacker-controlled parent directories, or an already compromised process with the same user permissions.

Banner contents are private by default. Titles, repository names, and actors are sent only after content previews are explicitly enabled. The operating system may store notifications, display them on the lock screen, or expose them during screen sharing. Its notification settings apply in addition to Hush's settings.

Clearing history logically deletes visible events but retains IDs and cursors to prevent duplicates. Disconnecting the account also deletes tasks and cursors. Neither operation securely overwrites SQLite WAL files, free database pages, backups, or OS notification history. For highly confidential data, use disk encryption and an appropriate deletion and backup policy.

## Integrity and supply chain

Hush itself runs as the logged-in user. The Windows installer and Linux Flatpak installation are scoped to that user; the macOS package installs `/Applications/Hush.app` and may require administrator authorization. Packaging scripts install application files and platform launcher entries, with autostart enabled only on explicit request. Review the scripts before running them. Native Windows COM/AppID behavior and macOS signing and installation still require validation on the target systems.

The release workflow grants `contents: read` to validation and build jobs; only the publication job receives `contents: write` to attach installers to the release. Checkout credentials are not retained. GitHub Actions are pinned to specific commit SHAs. `Cargo.lock` is versioned, and CI uses locked dependency resolution for tests and builds. The release tag updates only Hush's package version in the build checkout, preserving dependency versions and checksums. The validation stage runs `cargo audit` before platform builds; consult the actual workflow results for each release rather than assuming an audit passed. The MIT license covers Hush's own code; dependencies retain their respective licenses.

## Release validation

Validate Rust builds, tests, and Clippy on every target; the dependency audit; permissions in GitHub's authorization UI; OAuth cancellation, token expiration, renewal, and revocation; delivery on Niri/DMS, Windows, and both macOS architectures; locked and unlocked credential stores; rate-limit, offline, and restart behavior; and account disconnection during active requests. Complete the relevant target-system checks before using Hush with sensitive repositories. Do not claim that the application is free of CVEs, completely secure, or guaranteed to deliver every notification.
