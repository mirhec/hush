# User interface

Hush displays GitHub notifications as a single chronological inbox list. The window starts at 440 × 640 pixels, with a minimum size of 360 × 480. There is no sidebar, archive, or detail view. The application UI remains in German.

## Visual system

Neutral graphite backgrounds use mint for selection, focus, and received reviews. Purple identifies PR assignments and review requests, amber identifies mentions, and blue identifies new issues. Each action has its own vector icon: issue creation, PR assignment or review request, submitted PR review, and mention. Labels supplement the icons and color. The light theme uses light gray surfaces with the same semantic colors.

The header contains the inbox title, unread count, and settings icon on the right. Search and the filter button sit side by side below it. Each list row is 56 pixels high: the title appears above the repository, person, and time. The event icon and unread indicator sit on the left. Long text is truncated; a tooltip shows the full title. Headings use 18-pixel text, list titles 13, and metadata 11. Thin lines separate entries.

Custom vector icons are drawn with the egui painter. Native text uses the default fonts bundled with eframe; no extra font files or external font CDNs are required.

## Interaction

The inbox shows the 20 newest entries by default, including read notifications. The filter menu offers optional “Ungelesen” (unread) filtering, initially off, and filters by event type. Search covers the title, repository, person, and content. The list displays at most the 20 newest matches after applying search and filters. “Alle als gelesen markieren” (mark all as read) applies to the entire local history. Entries archived by earlier versions remain accessible as read history.

Clicking a row marks the entry as read locally without opening the browser. Hovering over a row reveals a separate GitHub button. The button is also reachable with the keyboard and visible when focused. It opens the entry in the system browser and marks it as read locally after a successful handoff. If the handoff fails, an unread entry stays unread and a toast shows the error.

Ctrl/Cmd+K focuses search. Escape closes the filter menu or returns from settings to the inbox. The back arrow in settings performs the same navigation. Unsaved settings are retained when switching views.

Settings are split across three tabs: “Benachrichtigungen” (notifications), “Konto” (account), and “Diagnose” (diagnostics). Event types and delivery options appear side by side at content widths of 520 pixels or more, and stacked below that. Toggle rows are at most 40 pixels high; supporting explanations appear in tooltips. Save and test-notification controls remain visible below the scroll area. Pause and resume controls sit under the refresh settings; manual refresh is available in diagnostics. The theme can be switched in the settings header.

The account tab offers browser-based GitHub sign-in. It shows the short device code and provides the verification link, with cancellation while authorization is pending. A brief explanation describes the repository permissions before login. Manual personal access tokens remain under the advanced options, alongside the team list. Sensitive inputs use password fields and are not serialized into GUI persistence. Diagnostics contains service controls and local data actions. System banners omit confidential repository and content details by default. Feedback appears as a toast for four seconds, or ten seconds for errors.

The [OAuth confirmation screen](screenshots/oauth-account.png) shows the browser sign-in flow with a synthetic device code.

## Checking the layout

The [inbox](screenshots/compact-inbox.png), [filter menu](screenshots/inbox-filters.png), [GitHub hover action at 360 pixels](screenshots/inbox-hover-small.png), and [small settings window](screenshots/compact-settings-small.png) images show actual egui drawing data with demo content. They were rendered without a desktop session from egui triangles and the corresponding font atlas. They demonstrate the layout, not native operating system interaction.

```sh
HUSH_UI_CAPTURE_DIR=/tmp/hush-ui cargo test --locked --lib export_native_ui_frames -- --ignored
```

Then open `scripts/render-ui.html` in a browser and select an exported JSON file. The renderer does not recreate the UI in HTML. The older HTML prototype in `preview/` is a separate historical design preview and no longer matches the current layout. Screen readers and platform-specific DPI settings still need testing on real target systems.
