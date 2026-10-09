# Hush

GitHub notifications for the desktop.

A desktop application built with **Rust + egui/eframe**, with a narrow GitHub
inbox, four event types, seven interface languages, and light or dark themes.

**Release versions from Git tags · MIT**

Local Linux builds and Rust tests have been validated. GitHub Actions has also
built Windows and macOS Apple Silicon installers; installation, keychain, tray,
and desktop notifications still need testing on the target systems. See
[VALIDATION.md](docs/VALIDATION.md) for recorded checks and remaining gaps.

## Application and preview

**The application:** `src/` contains the Rust implementation: UI, GitHub client,
event filters, background process, SQLite storage, and keychain/notification
integration. Current images of the native egui interface are in
`docs/screenshots/`; [DESIGN.md](docs/DESIGN.md) explains their export and rendering.

**The historical prototype:** `preview/hush-preview.html` and images in
`preview/screenshots/` show an earlier design with fictional data. Its detail
view, sidebar, and archive are no longer part of the application. The prototype
does not connect to GitHub or load external fonts or images.

## The four rules

| Rule | Implementation | Limit |
|---|---|---|
| A PR assigned to you / a review requested from you or your team | Assignment and review-request entries from the PR timeline, matched against your login and configured/discovered teams | GitHub must make the thread discoverable; team membership requires `read:org` or a manual list. |
| A new issue in selected repositories | Repository issue queries checked against `created_at`; PRs and subsequent updates alone are excluded | Up to 20 repositories; GitHub Projects boards are not supported. |
| A review on one of your PRs | Submitted reviews checked against the PR author; `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED` | Read access to the PR is required; the search index and API may lag. |
| A direct mention in a comment | Checks actual comment text in issues, PR comments, review comments/bodies, commit comments, and repository discussions | Only discoverable, accessible threads. This does not cover all of GitHub; Gists and organization discussions are excluded. |

GitHub's `mention` notification reason is explicitly **not** evidence of a new
mention. Hush reads the underlying events. Markdown code and quotations are
ignored when detecting mentions; this local analysis approximates GitHub's
mention parser rather than reproducing it exactly. Editing a previously reported
comment again does not produce another banner.

## Using Hush

The window starts at **440 × 640 pixels** and can shrink to **360 × 480**. The
inbox shows the **20 newest entries**, including read notifications. Beside
search, **Filter** opens the menu for optional unread-only filtering
(**Unread**), all or individual event types, and marking everything as read.
Unread-only filtering is off by default.
Search and filters show at most the 20 newest matching entries.

Each entry has an icon for its action: a new issue, a PR assignment or review
request, a submitted PR review, or a mention. Clicking the row marks it as read
locally. A separate button appears when hovering over the row or focusing the
button with the keyboard; it opens the entry on GitHub in the system browser.
Hush also marks the entry as read after a successful browser handoff. If opening
fails, an unread entry stays unread and a notice appears. Only
`https://github.com` links are accepted.

Read entries also show a **Mark as unread** button on hover or keyboard focus.
Use it to keep an entry for later: the unread state is saved locally across
restarts and appears in the optional **Unread** filter. The entry keeps its
original position by event time; marking it unread does not schedule a reminder
or send another desktop notification.

The gear at the top right opens settings with notifications, account, and
diagnostics tabs. `Ctrl+K` / `Cmd+K` switches to search; `Esc` closes the filter
menu or returns from settings to the inbox.

Hush supports **German, English, Spanish, French, Brazilian Portuguese,
Simplified Chinese, and Japanese**. It follows the system language by default,
with English as the fallback for other languages. The language selector in
settings lets you choose a language or return to the system default. Language
and light/dark theme changes apply and save immediately, including across
restarts; other settings use the **Save** button. Tray menus, application notices,
and desktop notification text follow the selected language. GitHub titles,
comments, repository names, account names, and your own input are not translated.

The main settings cover the four rules, issue repositories, system notifications,
private content previews, and a polling interval of 1/2/5 minutes. Pausing holds
back desktop notifications for 30 minutes while events continue to appear in the
inbox. There is no burst of delayed banners when the pause ends. The light/dark
theme can be switched with the icon in the settings header.

Chinese and Japanese font subsets are bundled for offline rendering, including
common Han characters and kana. No font download or system font installation is
needed. Rare ideographs outside the included ranges may not render. See the
[font documentation](assets/fonts/README.md) for coverage, attribution, and the
separate SIL Open Font License.

The initial sync imports at most the last 24 hours **without banners**. Stored
history is limited to 500 entries or 30 days; the list displays the 20 newest
matches. Older deduplication IDs are retained for up to 90 days.

**Read state is local:** Hush does not modify issues, PRs, or GitHub read state.
Multiple computers have independent inboxes and may show separate banners for
the same event. Entries archived in older versions remain in history as read.

## Connecting GitHub

Hush supports **github.com** and one GitHub account per operating-system user
profile. The default connection method is browser sign-in using GitHub's OAuth
device flow:

1. Open account settings and start GitHub sign-in.
2. Hush displays a short one-time code and opens GitHub in the browser.
3. Enter the code on GitHub and approve access. Hush completes the connection
   after GitHub confirms authorization.

A single OAuth login covers notifications and repository details, including
private repositories. No separate personal access tokens are needed. Credentials
are stored in the operating system's keychain, including a refresh token when
GitHub provides one. Expiring access tokens are renewed through that refresh
token, and rotated credentials are saved back to the keychain. There is no
embedded OAuth client secret, local callback server, or Hush authentication
server.

The requested scopes are `notifications`, `repo`, `read:org`, and `offline_access`.
GitHub's `repo` scope includes write access; GitHub does not offer an equivalent
OAuth scope restricted to reading private repositories. **Hush only reads
repository data** and keeps notification read state local. `read:org` supports
team discovery, and `offline_access` supports access-token renewal. Organization
policies, OAuth application restrictions, or SSO may require additional approval.
Revoked access or an expired refresh token requires signing in again.

### OAuth application setup for maintainers

Official Hush builds include the public client ID of the registered Hush OAuth
application. Users do not need to register an application or create tokens.
Device Flow is enabled for that registration.

For a fork or a separately registered application, override the public client ID
at build time using `HUSH_GITHUB_CLIENT_ID`. The release workflow accepts an
optional GitHub Actions repository variable with the same name; Flatpak staging
passes this public value into its build sandbox. For a local build:

```sh
HUSH_GITHUB_CLIENT_ID=YOUR_PUBLIC_CLIENT_ID cargo build --locked --release
```

An empty override uses the built-in Hush client ID. Never include a client secret.
See [RELEASING.md](docs/RELEASING.md) for release configuration.


### Advanced: personal access tokens

The previous token-based setup remains available as an advanced alternative for
users who want narrower repository permissions or already have configured tokens.

**Notifications token:** Create a separate, expiring **classic personal access
token** with only `notifications`. Optionally add `read:org` for automatic team
discovery. In this manual mode, other classic scopes, including `repo`,
`public_repo`, `gist`, and `user`, are rejected when connecting.

**Optional details token:** To read private repositories, add a **fine-grained
personal access token** for selected repositories with `Issues: Read-only`,
`Pull requests: Read-only`, and, if needed, `Discussions: Read-only`. Metadata
read access is part of GitHub's permission model. Commit comments may also need
`Contents: Read-only`. Do not add write or administration permissions.
Organizations may require approval.

The details token must belong to the same user. Hush checks identity and the
absence of reported classic scopes; **it cannot fully verify the fine-grained
permission matrix**. Check the read-only selections on GitHub. The application
itself performs only repository read operations even if an excessively privileged
details token is supplied.

Only one details token can be configured. A fine-grained token is limited to its
selected resource owner, so this manual mode may not cover several private
organizations at once. A details token can be added without re-entering the
stored notifications token. Enter tokens only in the account settings password
fields, never in command-line arguments, configuration files, or screenshots.
If avoiding `read:org`, enter team slugs manually in advanced settings, for example
`my-org/frontend`. The manual list is not verified against membership and must
be kept up to date.

### Repository access and delivery

Missing access appears in the repository-access section of settings and as a
notice in the main window. GitHub may report missing permissions for private
repositories as HTTP 404. Existing issues are imported without system banners;
new issues created afterward generate notifications. The test-notification button
checks the operating-system delivery channel independently of GitHub.

Keep GitHub's `Participating and @mentions` / `On GitHub` notifications enabled.
Hush uses the notification stream to discover threads. Selected issue
repositories are queried independently; this does not require enabling
`Watch → All Activity` for every repository.

## Running and building

Requirements: Rust through Rustup, a current C/C++ linker for your operating
system, and the Linux libraries listed below when applicable.
`rust-toolchain.toml` uses the stable toolchain channel; the project's declared
minimum is Rust 1.95. Dependencies are downloaded on the first Cargo invocation.

```sh
# Preview without tokens, network access, or local account data:
cargo run --locked --release -- --demo

# Native application with a real inbox:
cargo run --locked --release

# Validate before real use:
cargo test --locked --no-default-features
cargo check --locked --all-targets
cargo clippy --locked --all-targets
cargo build --locked --release
```

`Cargo.lock` is included. Build with `--locked` to use the validated dependency
versions. `eframe`, `keyring`, `notify-rust`, and `tray-icon` are also directly
pinned to specific versions.

### Linux, Niri, and Dank Material Shell

Wayland and X11 are enabled as eframe backends. Hush sends native notifications
through the existing Freedesktop/D-Bus service. If DMS provides that service,
no additional Dunst/Mako service is needed. Credentials require an unlocked
**Secret Service**, such as an appropriately configured GNOME Keyring or KWallet
with Secret Service support. Hush does not fall back to plaintext storage when
it is unavailable.

Example Debian/Ubuntu build dependencies; use equivalent packages on other
distributions:

```sh
sudo apt install build-essential pkg-config libdbus-1-dev libx11-dev libxi-dev \
  libxrandr-dev libxkbcommon-dev libwayland-dev libgl1-mesa-dev \
  libglib2.0-dev
cargo build --locked --release
bash packaging/linux/install.sh
```

The Linux tray uses StatusNotifier and D-Bus through GLib/GIO. Arch/CachyOS needs
`glib2`; AppIndicator is no longer required. Enable the system tray in the
Niri/DMS panel. If no tray host is reachable, the window remains accessible and
shows a notice.

Installation is per user in `~/.local/bin` and `~/.local/share`, without root
permissions. Start Hush from the app launcher or `~/.local/bin/hush`. The script
does not configure autostart.

For an **explicitly chosen** Niri autostart, adapt `packaging/linux/niri.kdl` to
your username. Do not blindly overwrite an existing Niri configuration.
`packaging/linux/dms-status.sh` is an optional JSON helper for a custom panel
widget, **not a ready-to-install DMS plugin**; it requires `jq` and publishes no
titles, repository names, or tokens.

```sh
~/.local/bin/hush --status
~/.local/bin/hush --stop
```

### macOS

Install Xcode Command Line Tools and Rustup, then run:

```sh
cargo build --locked --release
bash packaging/macos/bundle.sh
open dist/Hush.app
```

The script creates a locally/ad-hoc signed `.app` bundle with its own identifier,
but **no Developer ID signature or notarization**. Launch from this bundle for
native notifications and check operating-system permissions. Test notification
integration separately on the target macOS version. Do not disable Gatekeeper
or other system-wide protections.

Apple Silicon and Intel are built separately in the workflow, not as a universal
binary. After copying to `/Applications`, optional explicit autostart can be
configured with `bash packaging/macos/enable-autostart.sh`. It installs a user
LaunchAgent, not a system service.

### Windows

Use Rustup and Visual Studio C++ Build Tools. In PowerShell in the project folder:

```powershell
cargo build --locked --release
powershell -NoProfile -File packaging/windows/install.ps1
```

The script installs to `%LOCALAPPDATA%\Programs\Hush`, creates a Start menu entry,
and registers Hush's own AppUserModelID for Windows notifications. It does not
need administrator permissions or impersonate another application's identity.
If a PowerShell policy blocks the local script, inspect the script and its
source first; do not bypass organization-wide security policy.

Autostart is off by default and enabled only with the explicit `-AutoStart`
parameter. The release executable does not open a separate console window. CLI
output redirection/`--status` on Windows has not been tested; the status helper
is primarily intended for Linux.

### Builds and releases

The only workflow, `release.yml`, starts automatically only when a GitHub release
is published. It uses the tag's version, such as `1.0.2` or `v1.0.2`, for the
application and installers. `Cargo.toml` and `Cargo.lock` are changed only inside
the build directory; manual version edits or additional version commits are
unnecessary. The workflow validates packaging scripts and dependencies, tests
and builds Linux x86-64, Windows x86-64, macOS Apple Silicon and Intel, and uploads
all installers and SHA-256 checksums. Pushes to `main`, tag pushes, and pull
requests do not start Actions. An existing release can be rebuilt manually.
Signing and Apple notarization can be configured through GitHub secrets.
Without certificates, installers remain unsigned.

For Linux x86-64, the workflow creates a Flatpak bundle. Install it with
`flatpak install --user Hush-1.0.0-linux-x86_64.flatpak` and launch with
`flatpak run io.hush.github`. The Freedesktop runtime is downloaded from Flathub
if needed. Update by installing the bundle for the new version.

See [RELEASING.md](docs/RELEASING.md) for setup, filenames, and the release process.

## Window and background process

Normal startup opens the window, creates a tray/menu-bar icon, and starts a
separate background process if needed. On Linux, closing the window actually
ends its process; tray and service continue independently. Opening Hush from the
tray or launcher creates a new window with saved settings and events. On
Windows/macOS, the existing window is hidden and shown again. Separate file locks
prevent duplicate background processes and duplicate windows/tray icons. Without
an available tray, closing the window still leaves the background service running.

Left-clicking the tray icon opens the window or brings it to the front.
Right-clicking opens its menu. A dot on the tray icon indicates that at least one
local notification is unread, including entries manually marked unread. It
counts the entire retained history, independently of search, filters, and the
20-entry list limit, and disappears when all entries are read. The tray tooltip
shows service status and the exact unread count, without event titles or
repository names. Its actions open Hush or settings,
refresh, pause for 30 minutes/resume, start the service, or quit Hush. Quitting
ends the window, tray, and service once pending API requests finish. Stopping the
background service in settings or with `hush --stop` stops polling only; the
tray remains accessible.

- `hush --tray`: Start directly in the tray; show the window if no tray is
  available. The supplied autostart templates use this mode.
- `hush --start`: Start only the service and wait for startup confirmation.
- `hush --background`: Run the service in the invoking process, without a
  window/tray. Errors go to stderr.
- `hush --status`: JSON with `running`, `healthy`, `phase`, and `service_error`,
  alongside the other status fields.
- `hush --demo`: Offline demo without a service, tray, or account data.

Startup errors appear in the UI and when starting through `--start`. The private
data directory also contains `service.log` (normally
`~/.local/share/hush/service.log` on Linux); this error log is replaced on each
new service startup. On Linux, `window.log` may contain window-opening errors.
Process creation counts as successful service startup only after a confirmed
heartbeat. Concurrent SQLite writes from UI, worker, and heartbeat are serialized
before reading so they do not fail during a lock upgrade. File preparation also
preserves existing SQLite locks so the service and UI see the same data.

Logging out/restarting ends the processes; autostart requires explicit setup.
System banners still have no click action.

Delivery uses **polling, not server-side push**. The default is every two minutes,
subject to GitHub's minimum `X-Poll-Interval` and rate-limit backoff. Network
interruptions, search-index latency, large backlogs, and missing permissions may
delay or limit event detection.

## Scope and limits

GitHub Projects boards, Gists, organization-wide discussions, GitHub Enterprise
hosts, multiple accounts, and device synchronization are not supported. Hush
cannot guarantee mentions in threads absent from your GitHub notifications,
events that have since been deleted, or a complete historical import. A review
request to a team differs from assigning a PR to a group. Mention detection
focuses on direct personal mentions, not every team mention.

Requests are bounded: 8 MiB per response, at most ten pages per query, 160 API
calls per cycle, and 24 thread tasks per cycle. For notifications, the page limit
currently means at most 500 entries per complete query; for many other lists,
1,000. Very large threads/backlogs remain pending with a warning rather than
silently claiming a complete successful sync. A thread that exceeds the limit on
every retry is not automatically split.

There are no remote avatars, cloud relay, telemetry, or automatic application
updates in Hush's code. Corporate proxies are not inherited automatically.
See [SECURITY.md](docs/SECURITY.md) for further security boundaries.

## Project layout

```text
src/ui/          Native egui UI, design system, vector icons, offline demo
src/i18n.rs      Language selection, message lookup, and safe interpolation
src/locales/     Embedded translation catalogs for seven languages
src/api.rs       Bounded GitHub transport for repository read operations
src/oauth.rs     Browser device authorization and token refresh
src/filter.rs    Concrete event detection instead of sticky notification reasons
src/engine.rs    Background process, scheduling, retries, delivery
src/storage.rs   Private SQLite database, IDs, cursors, tasks, outbox
src/secrets.rs   Operating-system keychain; no plaintext fallback
src/notify.rs    Native banners, private by default
assets/fonts/    Bundled CJK font subsets and their SIL Open Font License
preview/         Historical HTML prototype
scripts/         Rendering of exported native egui interfaces
packaging/       Per-user installation and optional autostart
.github/         Release workflow with validation, builds, and installers
```

## Primary implementation references

GitHub: [Notifications](https://docs.github.com/en/rest/activity/notifications),
[Issue timeline](https://docs.github.com/en/rest/issues/timeline),
[PR reviews](https://docs.github.com/en/rest/pulls/reviews),
[Issues](https://docs.github.com/en/rest/issues/issues),
[Teams](https://docs.github.com/en/rest/teams/teams),
[GraphQL objects](https://docs.github.com/en/graphql/reference/objects),
[OAuth device flow](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow),
[Personal access tokens](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/managing-your-personal-access-tokens).

Desktop: [eframe 0.36.2](https://docs.rs/eframe/0.36.2/eframe/),
[keyring 3.6.3](https://docs.rs/keyring/3.6.3/keyring/),
[notify-rust](https://docs.rs/notify-rust/latest/notify_rust/),
[Dank Material Shell](https://danklinux.com/docs/dankmaterialshell/overview).
The original implementation references were retrieved or cross-checked on
October 8, 2026. Documentation and APIs may change.
