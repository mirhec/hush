# User interface

Hush displays GitHub notifications as a single chronological inbox list. The window starts at 440 × 640 pixels, with a minimum size of 360 × 480. There is no sidebar, archive, or detail view. The interface supports German, English, Spanish, French, Brazilian Portuguese, Simplified Chinese, and Japanese.

## Visual system

Neutral graphite backgrounds use mint for selection, focus, and received reviews. Purple identifies PR assignments and review requests, amber identifies mentions, and blue identifies new issues. Each action has its own vector icon: issue creation, PR assignment or review request, submitted PR review, and mention. Labels supplement the icons and color. The light theme uses light gray surfaces with the same semantic colors.

The header contains the inbox title, an unread-count badge, and settings icon on the right. Search, with an inset magnifying-glass icon, and the filter button sit side by side below it. Each list row is 56 pixels high: the title appears above the repository, person, and time. The action icon sits in a softly tinted tile, with an unread indicator below it. Read titles use muted text. Long text is truncated; a tooltip shows the full title. Headings use 18-pixel text, list titles 13, and metadata 11. Thin lines separate entries, while hover and keyboard focus add a rounded row surface and visible action controls.

Custom vector icons are drawn with the egui painter. The default eframe fonts handle Latin text. Two bundled, modified Noto Sans CJK subsets provide Chinese and Japanese glyphs offline, with Japanese forms selected for the Japanese interface. They include the translation catalogs, native language names, common Han characters, kana, and related punctuation, but omit rare ideographs in CJK extension blocks. The font assets total about 9.6 MiB and are compiled into the executable, so the interface needs no external font service or system font installation. [Font attribution and coverage](../assets/fonts/README.md) and the [SIL Open Font License](../assets/fonts/OFL.txt) are included in the repository and packaged with installers.

## Interaction

The inbox shows the 20 newest entries by default, including read notifications. The filter menu offers optional unread filtering, initially off, and filters by event type. Search covers the title, repository, person, and content. The list displays at most the 20 newest matches after applying search and filters. Marking all entries as read applies to the entire local history. Entries archived by earlier versions remain accessible as read history. Empty results show a short explanation and, when filters are active, a button to clear them.

Clicking a row marks the entry as read locally without opening the browser. Hovering over a row reveals a separate GitHub button. The button is also reachable with the keyboard and visible when focused. It opens the entry in the system browser and marks it as read locally after a successful handoff. If the handoff fails, an unread entry stays unread and a toast shows the error.

Ctrl/Cmd+K focuses search. Escape closes the filter menu or returns from settings to the inbox. The back arrow in settings performs the same navigation. Unsaved settings are retained when switching views.

Settings are split across three tabs: notifications, account, and diagnostics. Event types and delivery options appear side by side at content widths of 520 pixels or more, and stacked below that. Toggle rows are at most 40 pixels high; supporting explanations appear in tooltips. Save and test-notification controls remain visible below the scroll area. Pause and resume controls sit under the refresh settings; manual refresh is available in diagnostics. The theme can be switched in the settings header.

A language selector sits above the settings tabs and lists each language by its native name. The default follows the system language and falls back to English for unsupported languages. Language and theme changes apply immediately and persist across restarts without saving unrelated settings. Tab and action rows can wrap at narrow widths; descriptions stay in tooltips to preserve the compact layout. Tray text, application notices, and desktop notification labels use the chosen language. GitHub content and user input remain in their original language.

The account tab offers browser-based GitHub sign-in. It shows the short device code and provides the verification link, with cancellation while authorization is pending. A brief explanation describes the repository permissions before login. Manual personal access tokens remain under the advanced options, alongside the team list. Sensitive inputs use password fields and are not serialized into GUI persistence. Diagnostics contains service controls and local data actions. System banners omit confidential repository and content details by default. Feedback appears as a toast for four seconds, or ten seconds for errors.

The [OAuth confirmation screen](screenshots/oauth-account.png) shows the browser sign-in flow with a synthetic device code.

The [English inbox](screenshots/inbox-english.png), [Japanese settings](screenshots/settings-japanese.png), and [Chinese sign-in screen](screenshots/account-chinese.png) demonstrate translated layouts at 360 × 480 pixels.

## Checking the layout

The [inbox](screenshots/compact-inbox.png), [filter menu](screenshots/inbox-filters.png), [GitHub hover action at 360 pixels](screenshots/inbox-hover-small.png), and [small settings window](screenshots/compact-settings-small.png) images show actual egui drawing data with demo content. They were rendered without a desktop session from egui triangles and the corresponding font atlas. They demonstrate the layout, not native operating system interaction.

```sh
HUSH_UI_CAPTURE_DIR=/tmp/hush-ui cargo test --locked --lib export_native_ui_frames -- --ignored
```

Then open `scripts/render-ui.html` in a browser and select an exported JSON file. The renderer does not recreate the UI in HTML. The older HTML prototype in `preview/` is a separate historical design preview and no longer matches the current layout. Screen readers and platform-specific DPI settings still need testing on real target systems.
