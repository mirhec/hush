# Architecture and deliberately limited semantics

## Processes

A single executable provides a window mode and a `--background` mode. The window starts the background process using the absolute path of its own executable, without a shell. An exclusive file lock prevents duplicate polling processes. The window and worker each open their own SQLite connection. There is no localhost HTTP server, open network port, or token transfer in IPC messages. The worker reads credentials directly from the operating system's credential store.

A heartbeat updates a timestamp every five seconds. The worker checks stop and refresh flags between cycles; a running cycle can contain multiple requests, each with a 20-second timeout. Stopping the worker therefore does not immediately interrupt an active HTTP request. The worker is not an automatically installed system service and does not restart after a reboot unless autostart has been explicitly configured.

On Linux, a separate desktop process owns the StatusNotifier tray through GLib/GIO and starts the window as a child process (`--window`, an internal mode). The tray lock prevents duplicate desktop processes; the window lock prevents duplicate windows. A normal close command exits the window process while the tray and worker keep running. The tray and launcher open a new window when needed. This keeps Hush usable natively on Wayland/Niri: [`winit::Window::set_visible`](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.set_visible) does not support Wayland. Intercepting a close command and then sending `Visible(false)` would leave the window open and impossible to close normally there.

The Linux tray exports `org.kde.StatusNotifierItem` with `ItemIsMenu=false`. `Activate` opens the window; `com.canonical.dbusmenu` provides the right-click menu. Registration uses the object path and unique D-Bus sender rather than a PID-based service identifier. Restarting the tray host triggers registration again. ARGB pixmaps are transferred over D-Bus, so Flatpak does not need access to private image files. Windows and macOS use `tray-icon` with the left-click menu disabled and a handler for the primary click.

On Windows and macOS, the window still owns the native tray; tray actions wake `App::logic` even when the window is hidden. On Linux, SQLite flags carry open, settings, and quit requests; pause and service status are read from the database. On Linux, `--tray` starts only the tray and worker without creating a window first; `--background` remains a worker-only mode on every platform. If the Linux StatusNotifier host is missing or disappears, a window opens. An explicit quit request asks the window and worker to exit before removing the tray. Simply closing the window leaves the worker running; reopening the window does not automatically restart a worker that was stopped earlier.

All SQLite transactions that will write begin with `BEGIN IMMEDIATE`. Previously, `BEGIN DEFERRED` could fail immediately with `database is locked` when upgrading from a read to a write, despite the busy timeout, particularly during the concurrent first heartbeat. The heartbeat is initialized synchronously before startup is confirmed and is stopped before the worker lock is released. An additional file lock serializes startup requests; they check process exit and the heartbeat, and capture stderr in the private `service.log`. Worker errors also remain in the runtime status.

Preparing an existing database does not open and close an additional file descriptor. On POSIX, closing it would release the SQLite locks held by all threads in that process. When the heartbeat connection opened, this could let another process remove the still-active WAL file, leaving the worker and UI with separate states. The database is opened outside SQLite only when it is first created; existing files are checked through metadata and assigned private permissions. Background: [SQLite, POSIX advisory locks](https://www.sqlite.org/howtocorrupt.html#_posix_advisory_locks_canceled_by_a_separate_thread_doing_close_).

## Authentication

The default login uses GitHub's OAuth device flow. Hush requests a device code, opens GitHub's verification page in the system browser, and shows the short code for the user to enter. It polls for authorization with GitHub's required interval and handles cancellation, expiration, and slower polling requests. This flow needs a registered OAuth app with device flow enabled and its public client ID; it does not require a client secret or a Hush server.

The requested scopes are `notifications`, `repo`, `read:org`, and `offline_access`. One OAuth authorization covers notifications, private repository details, and team discovery. GitHub's `repo` scope also grants write permissions, although Hush only performs read operations against the repository API. `offline_access` requests an expiring access token and a refresh token. Both credentials are kept in the OS credential store, and token renewal keeps background polling working after the access token expires.

Manual personal access tokens remain available as an advanced alternative. That mode retains a narrowly scoped classic token for notifications and an optional fine-grained token for private repository details. Existing token-based accounts remain compatible. The security implications of the two modes are described in [SECURITY.md](SECURITY.md).

## Event sources

1. The Notifications API discovers updated threads, including those GitHub has already marked as read. Hush then fetches actual timeline, comment, and review entries instead of blindly treating `reason=mention` as an event.
2. An additional search for the user's recently updated pull requests discovers reviews. The search is limited by the token's visibility and GitHub's search index. It does not guarantee complete results in real time.
3. Selected issue repositories are queried directly. Only the creation time counts; new comments and pull request entries do not count as new issues.
4. With `read:org`, teams are refreshed roughly every 15 minutes. Manual `org/slug` entries are added to that list; organization changes may not become visible until the next refresh. Manual entries are not verified memberships.

GitHub's own delivery and subscription rules still affect notifications. Users should keep “On GitHub” enabled for participation and mentions. A missing, deleted, or inaccessible thread cannot be reconstructed. Unsupported types are collected as warnings instead of being converted into unreliable fallback mentions.

## Time, retries, and local identity

Source cursors advance only after a complete, successful discovery pass; thread tasks are persisted first. The initial scan looks back 24 hours and remains silent. Subsequent runs overlap by two minutes. New and retried thread tasks are merged. If a silent initial import and a live run overlap, the newer boundary takes precedence to avoid a flood of old banners; this can omit historical events if the initial import was incomplete.

A concrete event ID, such as a review ID or comment ID with a namespace, prevents repeated notifications for the same activity. This deliberately differs from a notification thread's mutable timestamp. Another newly submitted review has a different ID. Editing a previously reported comment does not create a new event. Edits that add a mention to a comment not previously reported can be detected through `updated_at`.

Read state is entirely local. A newer delivery gets its own event entry. Hush does not write to GitHub or synchronize state between devices. The former archive feature has been removed. The old SQLite `archived` column remains for compatibility; those entries are loaded as read items in the shared history. Old JSON payloads remain readable; new payloads have no archive field.

## Inbox and navigation

The UI consists of an inbox list with search and a filter menu. The window starts at 440 × 640 pixels, with a minimum size of 360 × 480. The list shows the 20 newest matching entries, including read entries, when the app opens. Unread-only filtering is optional and disabled by default. Search and event-type filters are applied before sorting by event time and limiting the results. The menu also lets users select all event types or one specific type, and mark the entire history as read.

Clicking an entry saves its local read state without opening the browser. A separate GitHub button appears on hover or keyboard focus. That action validates the GitHub link and passes it to the system browser, then saves the read state after a successful invocation. If opening fails, an unread entry remains unread and a toast reports the error. A successful invocation confirms the handoff to the browser, not that the GitHub page loaded.

The gear icon in the upper right opens settings with the “Benachrichtigungen” (notifications), “Konto” (account), and “Diagnose” (diagnostics) tabs. `Ctrl+K` / `Cmd+K` focuses search; `Esc` closes the filter menu or returns from settings to the inbox. The UI has no separate detail view, sidebar, or archive view.

## Errors and limits

A budget of 160 requests and 24 thread tasks per cycle limits load. Each page contains at most 50 notifications or, for most other objects, 100 items, with a safety limit of ten pages. If that limit is reached without proving completeness, the source cursor does not advance or the thread remains pending for retry. Extremely large threads are not yet split into time ranges automatically. Network and authentication errors, incomplete search results, unreadable private repositories, and API limits are reported in the status rather than silently treated as “no events.”

GitHub's `X-Poll-Interval`, rate-limit reset, and `Retry-After` determine longer pauses. The app is a polling client, not a webhook server. The fixed API origin and redirect prohibition also mean that Enterprise hosts are unsupported and moved or renamed repositories may cause errors.

## Delivery

Insertion, account checks, and deduplication are transactional. A persistent outbox is acknowledged after a successful call to the OS notification backend. A crash between OS delivery and acknowledgement can duplicate a banner: **there is no exactly-once guarantee**. A successful OS call also does not prove that the user saw a banner; focus mode or system permissions may prevent it from appearing.

Initial imports, disabled rules, disabled desktop notifications, and quiet periods do not produce a later replay flood. Larger batches are grouped into a private summary notification. If a user changes rules or pause settings during a request, the current settings are read again before delivery.

## Extensions

Complete server-side event delivery across many organizations would call for a GitHub App with explicit installations and signed webhooks. This requires infrastructure and a different trust and permission model; Hush does not include it. The OAuth app used for browser sign-in is not a GitHub App installation. Additional detail tokens for multiple resource owners, notification actions, and ETag caching are not implemented yet. Signed distributions require the certificates and secrets described in [RELEASING.md](RELEASING.md).
