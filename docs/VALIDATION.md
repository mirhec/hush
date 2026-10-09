# Validation record — October 9, 2026

This document records checks performed at specific implementation stages. Older
results describe the code and environment at that time, including features that
have since changed. They are not a claim that every check was rerun against the
latest revision.

## Cross-platform login startup

- **110 Rust tests passed** with `cargo test --locked --offline --all-targets`, including explicit-only startup changes, error/pending UI states, portal permission results, desktop-file escaping and precedence, stale/desktop-disabled Linux registrations, Windows command encoding, and atomic LaunchAgent writes.
- **11 packaging tests passed**, including Windows upgrade/uninstall registration behavior. Shell syntax and seven-language catalog keys/placeholders were checked.
- Clippy passed with the same five existing suggestions, and the Linux release build passed.
- The Windows backend and its tests were typechecked for `x86_64-pc-windows-msvc` in isolation. The macOS Core Foundation backend/tests were typechecked on Linux with framework linking disabled; this is not a macOS build or runtime test.
- UI tests cover native and portal controls at 360 × 480 in all seven languages, including a pending system request without blocking or duplicating writes. Native egui captures were inspected for the English native control and German/Japanese portal controls.

No autostart registration was changed in the actual user profile. File tests use temporary directories and UI tests inject a fake backend. Actual logout/login on each OS, macOS background-item approval, Windows Task Manager overrides, and a live Flatpak Background portal dialog remain target-system checks. The isolated D-Bus test cannot run in this sandbox because Unix socket binding is prohibited.

## Unread tray indicator and mark-as-unread action

- **97 Rust tests passed** with `cargo test --locked --offline --all-targets`. New coverage checks tray badge transitions and ARGB pixmaps, persistent unread state without replaying desktop notifications, legacy archived entries, independent row/GitHub/unread actions, keyboard activation, and narrow layouts.
- Clippy passed with the same five existing suggestions; the Linux release build passed.
- Native egui captures of the mark-as-unread action were inspected at 360 × 480 in dark and light themes. The generated tray pixels were inspected at small panel sizes on light and dark backgrounds.
- The isolated D-Bus integration test now checks `NewIcon` signals and exported pixels for read → unread → read transitions. It could not run here because the sandbox prevents the isolated bus from binding a Unix socket. Native Niri/DMS, Windows, and macOS tray rendering still needs target-desktop validation.

## Multilingual UI and appearance

- **92 Rust tests passed** with `cargo test --locked --offline --all-targets`, including seven-language catalog/placeholder coverage, safe diagnostic translation, persisted appearance without saving unrelated drafts, language changes in tray menus, notification preview privacy, and narrow translated settings with CJK glyph coverage.
- **10 packaging tests passed**; shell syntax and documentation links were checked. The CJK fonts are bundled, and their OFL license/attribution is included in native packages.
- Clippy passed with the five existing suggestions outside the localization work. The Linux release build passed; core tests also run without the desktop feature.
- The font subset script regenerated both fonts byte-for-byte; common Han characters, kana, punctuation, and all catalog characters were verified.
- Native egui captures covered all seven languages at 360 × 480. Visual inspection included German/English inboxes, Portuguese/Japanese settings, French/Chinese device sign-in, and the light theme.

These checks use synthetic inbox data and rendered egui drawing data. They do not demonstrate native Windows/macOS installers, OS language detection on those systems, real desktop notification delivery, or a live browser sign-in. The isolated D-Bus test remains skipped locally.

## Recent inbox and separate GitHub action

- **81 Rust tests passed** with `cargo test --locked --offline --all-targets`.
  Regression coverage includes the 20 newest matching entries regardless of read
  state, optional unread filtering, row clicks persisting read state without a
  browser call, separate GitHub clicks at 360/440 pixels, failed or unsafe links,
  and Tab/Enter/Space navigation of the independent row and GitHub actions.
- Clippy passed with the same five existing suggestions outside this change.
  The local Linux release build passed.
- Native egui captures were exported separately. The default 440 × 640 inbox,
  filter menu, and 360 × 480 hover action were inspected; the corresponding
  [screenshots](DESIGN.md#checking-the-layout) were updated.

Browser calls in interaction tests are substituted; this does not verify an
actual system-browser launch. The isolated D-Bus test remains skipped locally.
No native Windows/macOS interaction or installer build was run for this change.

## OAuth browser login and English documentation

- **78 Rust tests passed** with `cargo test --locked --offline --all-targets`,
  including nine device-flow protocol tests, cancellation after token delivery,
  token expiry handling, OAuth/manual scope separation, and concurrent account,
  preference, and pause updates. The isolated D-Bus test remains skipped locally;
  native UI capture is run separately.
- **10 packaging tests passed**, including forwarding the public OAuth client ID
  into the Flatpak build without copying access tokens or other environment data.
- Clippy and the local Linux release build passed. The five existing Clippy
  suggestions outside the new OAuth implementation remain.
- The account confirmation screen was rendered from actual egui geometry at
  360 × 480 and inspected with a synthetic device code. See the
  [OAuth account screen](screenshots/oauth-account.png).
- README, technical documentation, and code comments use English. Product UI
  text remains German. Historical test reports retain their original results.

The public Hush OAuth client ID is configured. A live request to GitHub's device
code endpoint with the requested scopes returned HTTP 200, a valid GitHub
verification URL, and the polling interval, confirming that the registration
accepts Device Flow. No user authorized that disposable verification request.

**A complete live sign-in and renewal have not been tested.** Protocol tests use
synthetic responses and do not prove organization approval, SSO, real credential
storage, or browser authorization on any target OS. Existing manual-token
connections remain compatible.

## Narrow inbox

- **59 Rust tests passed** with `cargo test --locked --offline --all-targets`.
  Coverage includes unread-only defaults, the interaction between the filter
  menu and search, click → GitHub → persisted read state, entries remaining
  unread after browser/link errors, marking all entries as read, and
  compatibility with previously archived data.
- Layout and interaction tests cover 360- and 440-pixel window widths,
  56-pixel rows with long titles, visible settings actions at 360 × 480, and
  preserving drafts when switching tabs or pages.
- Clippy passed with five pre-existing warnings; the Linux release build
  succeeded. The executable is at `target/release/hush`.
- Eleven views were exported from the actual egui drawing data. Visual checks
  covered the narrow inbox in light/dark mode, the filter menu, settings at
  440 × 640 and 360 × 480, and account/diagnostics views at 360 × 480.
  The export test ran separately; the isolated D-Bus test remained skipped
  because of the local sandbox.

[Images and instructions for repeating the layout check](DESIGN.md#checking-the-layout).
The images use actual egui geometry and font textures with demo content.
Browser handoff and local read-state persistence were tested with a substituted
browser call and a temporary database. A native desktop session, actual browser
launch behavior, and different operating-system scale factors were not tested
here. The running installation and personal account data were not changed for
this revision.

## Release version derived from the Git tag

The release workflow applies the tag's version to the Cargo files in each build
directory. The checked-in development version no longer needs to be updated for
each release. The change is not committed back.

- **Nine packaging checks passed**, including stamping `1.0.1`, `v1.0.2`, and
  prereleases, repeated execution, unchanged dependencies and checksums,
  rejection of invalid tags before file changes, and export of installer
  versions through `GITHUB_ENV`.
- In a separate source copy, the Cargo version was changed from `1.0.0` using
  tag `v1.0.2`. All nine packaging checks passed again there, including Flatpak
  source preparation with matching Cargo and AppStream versions.
- `cargo metadata --locked --offline --no-deps` accepted the prepared files.
  The application built there with
  `cargo run --locked --offline --no-default-features` returned **`Hush 1.0.2`**
  when called with `--version`.
- Workflow YAML, shell syntax, release-only triggers, job dependencies, and
  stamping before Cargo in both build directories were checked.

At this validation stage, the complete new installer pipeline had not yet run
on GitHub. Its first run required a new release on the updated `main`; older
tags still contained the previous version check.

### Previous release failures

The [1.0.0 release run](https://github.com/mirhec/hush/actions/runs/37794586738)
failed because tag `1.0.0` differed from Cargo version `0.1.0`. After they were
aligned, the same error occurred with tag `1.0.1` versus Cargo version `1.0.0`.
Automatically applying the tag fixes that cause.

When the earlier [main-branch run](https://github.com/mirhec/hush/actions/runs/37794473971)
was inspected, Windows and macOS Apple Silicon, including packaging, had
succeeded; Linux and macOS Intel were still running. The isolated Linux D-Bus
test passed. These results do not demonstrate manual installation or desktop
interaction on the target systems.

## Earlier stage: UI, tray click, releases, and notifications

- **52 Rust tests passed** with `cargo test --locked --offline --all-targets`.
  They included fixed settings widths and toggle alignment at 360/540/740 px,
  mouse/keyboard interaction, toast expiry, StatusNotifier activation and D-Bus
  menu types, and importing three new issues into the delivery queue.
- The additional real D-Bus integration test was **skipped** locally: the sandbox
  allowed neither a private D-Bus socket nor the Flatpak network namespace
  (`Operation not permitted`). Linux CI explicitly runs it in `dbus-run-session`;
  it checks clicks, menus, host restart, and unregistration.
- **5 packaging checks passed**: version/tag/lockfile consistency, prerelease
  metadata, invalid versions, app identity, and isolated Flatpak source
  preparation, including permissions and release metadata.
- Workflow YAML, shell syntax, the desktop file, and preview JavaScript were
  checked.
- **23 browser checks** of the updated HTML preview passed through Chrome
  DevTools, including aligned toggles, search/archive, the private banner
  preview, and automatically disappearing toasts. These were not native egui
  screenshots. The earlier Playwright report below describes the original
  implementation.
- The Linux release executable `target/release/hush` was rebuilt. Clippy passed
  with warnings in existing code; Windows/macOS builds and GitHub Actions were
  not run locally. Native installations and the Flatpak package build still
  needed confirmation on the target systems or in CI.

The running installation was diagnosed through read-only access. The background
service had a current heartbeat, but all five configured issue repositories
returned HTTP 404. No details token was configured, and no issue had been
imported. Names, content, and credentials were not included in this document.
That implementation exposed repository access errors directly in the UI and
allowed a details token to be added without re-entering the stored notifications
token. Deferred thread requests were then released immediately. Delivery errors
were cleared only after actual successful delivery or a successful test, not
when the queue was empty. Tests covered 404 responses without cursor progress,
three new issues after a silent initial import, failed/successful delivery, and
deduplication.

At this stage, successful access to private GitHub repositories and actual
desktop banners remained unconfirmed pending a details token with sufficient
permissions and delivery testing in the desktop session. This describes the
historical token-based account setup.

## Earlier checks during initial creation

**25 browser checks passed** against the separate `preview/hush-preview.html`
using headless Chromium through Python Playwright. Full list:
[preview-test-results.json](preview-test-results.json). Coverage included search
and empty states, filters, unread counts, selection/read state, archive/undo,
keyboard interaction, pause, light/dark mode, settings, repository validation,
simulated private banners, and a 1000-pixel window. No JavaScript runtime errors
or network requests were recorded for this preview.

**13 static/format checks passed:** both TOML files and both workflow YAML files
parsed successfully, four shell scripts passed `bash -n`, the macOS plist
parsed successfully, JavaScript passed `node --check`, the SQLite schema from
`storage.rs` was created in Python SQLite, no `todo!`/`unimplemented!` macros
were present in Rust application code, and no additional font files were
bundled. Full list: [static-test-results.json](static-test-results.json).

The four screenshots in `preview/screenshots/` were generated from the HTML
file. The dark inbox and settings were visually checked for readability,
positioning, and obstructive overlaps. These images do **not** validate native
egui window rendering.

## Rust validation after the tray and service fixes

Checked on October 8, 2026, on Linux x86_64 with Rust 1.99.0. Tray dependencies
were added to the existing `Cargo.lock`; Cargo accepted it with `--locked`.

The previous service startup failure was reproduced with a fresh isolated data
directory: the process exited with `Hush: database is locked`, without a token
or GitHub access. The new concurrency test failed at the same point before the
fix and passed with `BEGIN IMMEDIATE`. It deliberately holds a competing SQLite
write open and checks control commands and settings changes. The additional
process test exposed a second issue: opening/closing the database outside
SQLite released POSIX locks; after a separate startup, the worker did not
reliably observe stop commands. The test therefore also checks actual startup,
continuous heartbeats, competing writes, duplicate workers, stopping, and
restarting.

- `cargo test --locked --offline --no-default-features`: 38 tests passed
  (7 unit, 27 event/storage, and 4 new service tests).
- `cargo test --locked --offline --all-targets`: 42 tests passed, including
  four additional Linux window/tray regression tests.
- `cargo check --locked --offline --all-targets`: passed with tray dependencies.
- `cargo clippy --locked --offline --all-targets`: passed; existing warnings
  about older UI/filter/test code remained.
- `cargo build --locked --offline --release`: passed; native Linux executable
  at `target/release/hush`.
- Final test of the actual release executable with a fresh data directory:
  startup confirmation, status, a second worker, stopping, restarting, and stale
  heartbeat detection passed; no credentials or GitHub access were used.
- Shell syntax of Linux/macOS packaging scripts: passed.

## Niri/Wayland fix

The previous close path sent `CancelClose` and `Visible(false)`. The winit
version in use, 0.30.13, ignores `set_visible` on Wayland, so Niri could not close
the window. On Linux, an independent desktop process now owns the tray. The
window process allows the native close command; tray and worker remain alive.
The tray process can subsequently open a new window. `--tray` no longer needs a
hidden Wayland window.

Four new tests check the actual egui viewport commands during a close event
(neither `CancelClose` nor `Visible(false)`), persisted settings when recreating
the UI, pause/settings/quit from the controller, and reuse of existing tray and
window instances. They run without a desktop session and do not replace an
interactive Niri test. The rebuilt release executable was also tested without
a display: it attempted the window fallback, wrote its error to `window.log`,
the desktop process exited with an error status, and the service remained
independently healthy and could subsequently be stopped.

## Checks not performed in these local validation sessions

| Area | Status |
|---|---|
| Native tray menus, closing/opening, Niri/DMS host changes | Desktop session and private D-Bus sockets blocked by the sandbox; requires local manual testing |
| Windows/macOS builds and native interaction | Not run on this Linux system |
| Real GitHub events / SSO / team memberships | No credentials used in automated validation |
| Keychain and actual OS banners | Not tested |
| Installation scripts / autostart / macOS signing | Not run natively |
| GitHub Actions and `cargo audit` | Not run as part of these local sessions |

The Linux build and local tests do not establish live operation across all
platforms. The separate HTML preview does not test the native tray.

Manual tray check: start Hush normally, close it, and reopen it through the tray;
open it again from the launcher and check that exactly one window/icon exists.
Check pause/resume, refresh, service stop/start, and complete shutdown. With a
tray available, `--tray` should start without a window; without a StatusNotifier
host, the window should appear with a notice. Restart the desktop panel while
Hush is in the tray and check that it remains accessible. `--demo` must create
neither a tray nor a worker.

## Reproducible validation commands

Native Rust checks after installing build dependencies:

```sh
cargo test --locked --no-default-features
cargo check --locked --all-targets
cargo clippy --locked --all-targets
cargo build --locked --release
cargo run --locked --release -- --demo
```

Keep the included `Cargo.lock` and validate with `--locked`. Also document a
fixed Rust toolchain version for an actual release; the supplied `stable`
channel changes over time.

Browser check, independent of Rust:

```sh
# After separately installing Python Playwright and Chromium:
CHROMIUM=/usr/bin/chromium python tests/preview_smoke.py
```

The test uses `--no-sandbox` only for the headless browser in the isolated test
environment, not for the native Hush application. It does not install a browser
or contact any website.

## Native acceptance testing before using real private repositories

Start with a test account and a non-confidential test repository. Set up the
account, complete the initial import, then trigger each event **after** that
import: a new personal assignment, a direct review request, a request to a team
the account actually belongs to and to another team; a new issue versus a
comment on an old issue; a review on the account's own PR versus another user's
PR; a direct mention, a similar login, a code block, a follow-up comment without
a mention, and an edited comment. While paused, entries should appear but
banners should not.

Also check network disconnect/reconnect, simulated GitHub rate limits/429
responses, app/worker restart, duplicate background starts, revoked tokens, a
locked keychain, missing repository permissions, account disconnection during
sync, and local data access. Network tests should also confirm redirect/foreign
host blocking and the absence of proxy support.

On Niri/DMS, test the user D-Bus session and an unlocked Secret Service. On
Windows, launch from the registered Start menu entry. On macOS, launch from the
generated `.app` bundle and check permissions/Focus mode. Test every target
system separately; a Linux test does not demonstrate working Windows or macOS
delivery.
