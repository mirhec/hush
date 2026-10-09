# Releases and installers

`release.yml` is the only GitHub Actions workflow. It runs automatically only
when a GitHub release is published, including prereleases. Pushes to `main`, tag
pushes, and pull requests do not start Actions. The release version comes
exclusively from the selected Git tag. The workflow does not create additional
releases or choose the next version number.

All stages appear in a single "Release Hush" run:

1. Validate the tag as a version number and apply it to the Cargo files in the
   build directory; check packaging scripts; confirm that the published release
   exists; run `cargo audit`.
2. After validation succeeds, run tests, Clippy, builds, and installer packaging
   for all four platforms. Every build uses the same previously validated commit
   and applies the same release tag as its package version before compilation.
3. After every platform succeeds, upload the complete installer set and
   `SHA256SUMS.txt` to the release.

If a stage fails, its dependent stages do not run. An invalid version tag stops
the workflow before the expensive platform builds.

| Platform | Release asset | Installation |
|---|---|---|
| Windows x86-64 | `Hush-VERSION-windows-x86_64-setup.exe` | Inno Setup, per user, with uninstaller |
| macOS Apple Silicon | `Hush-VERSION-macos-arm64.pkg` | `/Applications/Hush.app` |
| macOS Intel | `Hush-VERSION-macos-x86_64.pkg` | `/Applications/Hush.app` |
| Linux x86-64 | `Hush-VERSION-linux-x86_64.flatpak` | Flatpak, per user |

Autostart is not enabled automatically. The Windows installer offers it as an
optional checkbox that is initially unchecked. Uninstalling preserves account
data and history; use Hush's disconnect-and-delete action first to remove them
if needed.

## Release process

1. Push the desired changes to `main`.
2. Publish a GitHub release with a new tag pointing to that revision, such as
   `1.0.2` or `v1.0.2`. The Cargo version does not need to be changed.
3. Check "Release Hush" in GitHub Actions. Installers appear on the release only
   after all builds have finished.

Tags must be valid SemVer versions, optionally prefixed with `v`. Prereleases
such as `v1.1.0-rc.1` retain their full name in the application and filenames.
Numeric Windows/macOS metadata uses `1.1.0` for that example. Each of the three
numeric components must be at most 65535 for the native installers. Invalid tags
are rejected before platform builds start.

`python3 packaging/version.py --stamp --tag "$RELEASE_TAG"` sets Hush's version
in `Cargo.toml` and its matching entry in `Cargo.lock`. Dependencies and checksums
remain unchanged; the lockfile is not resolved again, and no commit is written
back. Cargo also uses this version for `CARGO_PKG_VERSION`, keeping
`hush --version` and the UI consistent with the installer. Flatpak source
preparation includes the updated files, so its build receives the same version
without Git metadata or network access.

Local builds without this preparation step retain the checked-in development
version from Cargo. Calls without `--stamp` only read and validate the version;
packaging does not change it afterward.

A failed run can be restarted. "Run workflow" also accepts the tag of an existing
release. Uploads replace assets with matching filenames; immutable GitHub
releases do not allow this after publication. Release immutability would require
publishing after asset upload; this pipeline explicitly uses `published`.

The Flatpak pipeline installs the built bundle in the CI profile and checks the
application version, service startup, and status without GitHub credentials. A
separate test checks tray activation, its menu, and host restart in an isolated
D-Bus session.

### GitHub OAuth application

Browser sign-in uses the registered Hush OAuth application's device flow. Its
public client ID is included in `src/oauth.rs`; official builds work without an
extra repository setting. For a fork, register an OAuth application, enable
Device Flow, and set the optional repository Actions variable
`HUSH_GITHUB_CLIENT_ID`. The workflow applies this build-time override on every
platform, including the Flatpak sandbox. An empty override uses Hush's built-in
client ID. This identifier is public; never add an OAuth client secret to the
repository or installer.

The OAuth login requests `notifications`, `repo`, `read:org`, and `offline_access`
to cover notifications, private repositories, organization/team membership, and
renewal of expiring access tokens. GitHub's
`repo` scope includes write permissions even though Hush only reads repository
data. GitHub presents the permissions during browser authorization. Organization
policies and SSO can require additional approval.

### Failures in the original 1.0.0 and 1.0.1 releases

Tag `1.0.0` pointed to commit `1bf25da`, whose `Cargo.toml` and `Cargo.lock` still
contained `0.1.0`. All four release jobs therefore failed validation with
`Release tag '1.0.0' does not match Cargo version '0.1.0'`. The main-branch build
started at the same time had no release tag to validate and could continue. It
was independent of the release run and did not supply artifacts to it.

After the Cargo version was raised to `1.0.0`, the same error occurred with tag
`1.0.1`. The workflow now derives the release version from the tag. At the time
of the fix, the next suggested release was `1.0.2` on the updated `main`. Older
tags still contain the old script; "Re-run jobs" does not update their source
code. Existing published tags are not moved. Subsequent releases only need a
new tag.

## Signing

Without certificates, Windows/macOS installers are unsigned. The macOS app
bundle then has only an ad-hoc signature, without Apple notarization. Gatekeeper
or SmartScreen can warn about or block installation. For public distribution,
configure the following GitHub Actions secrets; never commit certificates or
passwords to the repository.

**macOS:**

- `MACOS_CERTIFICATE_P12`: Base64-encoded P12 containing the private keys for
  **Developer ID Application** and **Developer ID Installer**.
- `MACOS_CERTIFICATE_PASSWORD`: Password for that P12.
- `MACOS_APP_IDENTITY`: Full name of the Developer ID Application identity.
- `MACOS_INSTALLER_IDENTITY`: Full name of the Developer ID Installer identity.
- For notarization, also set `APPLE_ID`, `APPLE_TEAM_ID`, and
  `APPLE_APP_PASSWORD` (an app-specific Apple password).

The build imports the certificate into a temporary runner keychain, signs the app
and package, and removes the keychain even after errors. If Apple credentials
are configured, it waits for notarization and attaches the ticket to the package
with `stapler`. Signing errors fail the run.

**Windows:**

- `WINDOWS_CERTIFICATE_PFX`: Base64-encoded Authenticode PFX.
- `WINDOWS_CERTIFICATE_PASSWORD`: Password for the PFX.
- Actions variable `WINDOWS_TIMESTAMP_URL`: The certificate provider's RFC 3161
  timestamp URL.

SignTool signs the executable first, then the completed installer, and verifies
both signatures. The temporary PFX file is removed afterward. Hardware-bound or
cloud signing keys require integration with the relevant provider instead of
the PFX script. In this setup, the Inno uninstaller does not receive its own
Authenticode signature.

## Linux: Flatpak

The approved format is a **Flatpak bundle for x86-64**. The build uses
Freedesktop Platform/SDK 25.08 and the Rust SDK extension. Cargo dependencies
are vendored into an isolated source directory based on `Cargo.lock`; tests and
compilation inside the SDK run with `--frozen` and no network access. Source
preparation includes only the designated build files.

The manifest allows Wayland, X11 fallback, GPU access, network access to GitHub,
and targeted D-Bus connections for StatusNotifier, Secret Service, and desktop
notifications. It grants no access to the home directory or the entire session
bus. Data is stored under `~/.var/app/io.hush.github/data/hush/`, and tokens are
stored in the system keychain. Existing data from installations outside Flatpak
is not migrated automatically. Tray and service remain active after the window
closes; the desktop needs a StatusNotifier host and an unlocked Secret Service.

```sh
flatpak install --user Hush-1.0.0-linux-x86_64.flatpak
flatpak run io.hush.github
flatpak run io.hush.github --tray
flatpak run io.hush.github --status
```

The required runtime is downloaded from Flathub if needed. A bundle does not
provide an automatic update channel: install the new `.flatpak` file to update.
No Flathub listing is published. The Windows/macOS installers do not set up an
automatic application updater either.

A local package build requires `flatpak`, `flatpak-builder`, and the runtime:

```sh
flatpak --user remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak --user install flathub org.freedesktop.Platform//25.08 org.freedesktop.Sdk//25.08 org.freedesktop.Sdk.Extension.rust-stable//25.08
bash packaging/flatpak/package.sh
```

## Local validation

```sh
python3 -m unittest discover -s tests -p 'test_packaging.py'
python3 packaging/version.py
cargo test --locked --all-targets
```

For a local release build in a separate source copy, first run
`python3 packaging/version.py --stamp --tag v1.0.2`, then build with Cargo and
package the result. The command updates both Cargo files in that copy.

On macOS, after `cargo build --release`:

```sh
bash packaging/macos/package.sh
```

On Windows with Inno Setup 6, after `cargo build --release`:

```powershell
./packaging/windows/package.ps1
```

Test installers on their target systems for installation, updates, Start menu/app
identity, tray behavior, and notifications. During the original local validation
on Linux, Windows/macOS packaging tools and GitHub Actions were not run. The
Flatpak run was blocked by restricted sandbox sockets; its manifest and source
preparation were validated locally. These historical results do not replace
validation of a new release on each target system.

References: [GitHub release events](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#release),
[Inno Setup: per-user installation](https://jrsoftware.org/ishelp/topic_setup_privilegesrequired.htm),
[AppUserModelID in shortcuts](https://jrsoftware.org/ishelp/topic_iconssection.htm),
[Apple packaging](https://developer.apple.com/documentation/xcode/packaging-mac-software-for-distribution),
[Flatpak desktop integration](https://docs.flatpak.org/en/latest/desktop-integration.html).
