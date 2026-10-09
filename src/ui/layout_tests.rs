use super::*;

#[test]
fn settings_rows_keep_equal_width_and_align_switches_with_tooltips() {
    for width in [360., 540., 740.] {
        let ctx = egui::Context::default();
        theme::apply(&ctx, false);
        let mut rows = Vec::new();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            rows.clear();
            ui.set_width(width);
            for (title, help, icon) in [
                ("Reviews", "Deine Pull Requests.", Some(Icon::ReviewSubmitted)),
                ("Erwähnungen", "Neue Erwähnungen in Kommentaren zu Issues und Pull Requests in deinen ausgewählten Repositories.", Some(Icon::Mention)),
                ("Auf dem Desktop anzeigen", "Auch bei geschlossenem Fenster.", None),
            ] {
                let response = setting_toggle(ui, title, help, &mut true, Palette::dark(), icon);
                rows.push((response.response.rect, response.inner.rect));
            }
        });
        output.textures_delta.clear();
        for (card, toggle) in &rows {
            assert!((card.width() - width).abs() < 1., "width {width}: {card:?}");
            assert!(
                (toggle.right() - rows[0].1.right()).abs() < 1.,
                "switch alignment"
            );
            assert!(card.contains_rect(*toggle), "switch must stay in its card");
            assert!(
                (card.right() - toggle.right() - 10.).abs() < 1.,
                "right padding"
            );
        }
        assert!(rows.iter().all(|(row, _)| row.height() <= 40.));
        assert_eq!(
            rows[1].0.height(),
            rows[0].0.height(),
            "Help text stays in a tooltip"
        );
        assert!(rows[0].0.bottom() <= rows[1].0.top());
    }
}

fn demo_app(ctx: &egui::Context) -> HushApp {
    HushApp::new(
        &eframe::CreationContext::_new_kittest(ctx.clone()),
        None,
        None,
        Config::default(),
        true,
        false,
    )
}

fn draw(
    ctx: &egui::Context,
    app: &mut HushApp,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> Vec<(String, Rect)> {
    use eframe::App;
    fn texts(shape: &egui::Shape, result: &mut Vec<(String, Rect)>) {
        match shape {
            egui::Shape::Text(text) => result.push((
                text.galley.text().to_owned(),
                Rect::from_min_size(text.pos, text.galley.size()),
            )),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    texts(shape, result);
                }
            }
            _ => {}
        }
    }
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0., 0.), size)),
            events,
            ..Default::default()
        },
        |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
    );
    let mut labels = Vec::new();
    for shape in &output.shapes {
        texts(&shape.shape, &mut labels);
    }
    output.textures_delta.clear();
    labels
}

fn click_text(ctx: &egui::Context, app: &mut HushApp, size: egui::Vec2, title: &str) {
    let labels = draw(ctx, app, size, vec![]);
    let position = labels
        .iter()
        .find(|(text, _)| text == title)
        .unwrap_or_else(|| panic!("Missing {title}"))
        .1
        .center();
    for pressed in [true, false] {
        draw(
            ctx,
            app,
            size,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn narrow_inbox_defaults_to_all_recent_events_and_keeps_rows_compact() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    assert!(!app.unread_only);
    assert_eq!(app.visible_events().len(), 8);
    for width in [360., 440.] {
        let size = vec2(width, 640.);
        draw(&ctx, &mut app, size, vec![]);
        let labels = draw(&ctx, &mut app, size, vec![]);
        for event in app.events.iter().filter(|event| event.unread) {
            let rect = labels
                .iter()
                .find(|(text, _)| text == &event.title)
                .expect("Unread titles are present, even when elided")
                .1;
            assert!(Rect::from_min_size(pos2(0., 0.), size).contains_rect(rect));
        }
        for title in ["Ungelesen", "Erledigt", "Erledigen", "Alle Typen"] {
            assert!(
                !labels.iter().any(|(text, _)| text == title),
                "{title} must not be in the inbox"
            );
        }
    }
    let mut event = app.events[0].clone();
    event.title = "Ein sehr langer Issue-Titel ohne Platz für eine weitere Zeile. ".repeat(20);
    event.repository = "organisation/".to_owned() + &"repository".repeat(30);
    for width in [280., 336., 416.] {
        let mut rect = Rect::NOTHING;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(width);
            rect = event_card(ui, &event, Palette::dark()).row.rect;
        });
        output.textures_delta.clear();
        assert_eq!(rect.height(), 56.);
        assert!((rect.width() - width).abs() < 1.);
    }
}

#[test]
fn filter_menu_combines_unread_type_and_search() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let size = vec2(360., 480.);
    draw(&ctx, &mut app, size, vec![]);
    click_text(&ctx, &mut app, size, "Filter");
    assert!(egui::Popup::is_id_open(&ctx, filter_popup_id()));
    assert!(!app.unread_only);
    assert_eq!(app.visible_events().len(), 8);
    click_text(&ctx, &mut app, size, "Neue Issues");
    assert_eq!(app.kind, Some(Kind::Issue));
    assert_eq!(app.visible_events().len(), 2);
    app.search = "WAYLAND".into();
    assert_eq!(app.visible_events().len(), 1);
    click_text(&ctx, &mut app, size, "Ungelesen");
    assert!(app.unread_only);
    assert_eq!(app.visible_events().len(), 1);
    draw(&ctx, &mut app, size, vec![escape()]);
    assert!(!egui::Popup::is_id_open(&ctx, filter_popup_id()));
    assert!(app.page == Page::Inbox);
}

fn escape() -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Escape,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn inbox_shows_twenty_newest_matches_including_read_events() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let seed = app.events[0].clone();
    app.events = (0..50)
        .map(|index| {
            let mut event = seed.clone();
            event.id = index.to_string();
            event.occurred_at = seed.occurred_at - chrono::Duration::minutes(index);
            event.unread = index % 2 == 0;
            event.kind = if event.unread {
                Kind::Issue
            } else {
                Kind::Review
            };
            event.title = format!("{} {index}", if event.unread { "issue" } else { "review" });
            event
        })
        .rev()
        .collect();
    let visible = app.visible_events();
    assert_eq!(visible.len(), 20);
    assert_eq!(visible.first().unwrap().id, "0");
    assert_eq!(visible.last().unwrap().id, "19");
    assert!(visible.iter().any(|event| !event.unread));
    app.kind = Some(Kind::Review);
    app.search = "review".into();
    let visible = app.visible_events();
    assert_eq!(visible.len(), 20);
    assert_eq!(visible.first().unwrap().id, "1");
    assert_eq!(visible.last().unwrap().id, "39");
    app.unread_only = true;
    assert!(app.visible_events().is_empty());
    app.kind = None;
    app.search.clear();
    assert_eq!(app.visible_events().len(), 20);
    assert!(app.visible_events().iter().all(|event| event.unread));
}

fn click_github(ctx: &egui::Context, app: &mut HushApp, size: egui::Vec2, title: &str) {
    let labels = draw(ctx, app, size, vec![]);
    let title_rect = labels.iter().find(|(text, _)| text == title).unwrap().1;
    let position = pos2(size.x - 26., title_rect.top() + 10.);
    draw(ctx, app, size, vec![egui::Event::PointerMoved(position)]);
    for pressed in [true, false] {
        draw(
            ctx,
            app,
            size,
            vec![egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            }],
        );
    }
}

#[test]
fn github_action_opens_the_browser_and_marks_the_event_read() {
    thread_local! { static CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
    for width in [360., 440.] {
        let ctx = egui::Context::default();
        let mut app = demo_app(&ctx);
        app.open_url = |url| {
            assert_eq!(url, "https://github.com/notifications");
            CALLS.set(CALLS.get() + 1);
            Ok(())
        };
        CALLS.set(0);
        let title = app.events[0].title.clone();
        click_github(&ctx, &mut app, vec2(width, 640.), &title);
        assert_eq!(CALLS.get(), 1);
        assert!(!app.events[0].unread);
        assert_eq!(app.visible_events().len(), 8);
    }
}

#[test]
fn github_action_is_keyboard_reachable_and_does_not_activate_the_row() {
    let ctx = egui::Context::default();
    let event = demo::events().remove(0);
    let mut responses = None;
    let mut run = |events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_width(336.);
                responses = Some(event_card(ui, &event, Palette::dark()));
            },
        );
        output.textures_delta.clear();
        responses.take().unwrap()
    };
    let first = run(vec![]);
    ctx.memory_mut(|memory| memory.request_focus(first.row.id));
    let key = |key, pressed| egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    run(vec![key(egui::Key::Tab, true)]);
    let focused = run(vec![key(egui::Key::Tab, false)]);
    assert!(focused.github.has_focus());
    let clicked = run(vec![key(egui::Key::Enter, true)]);
    assert!(clicked.github.clicked());
    assert!(!clicked.row.clicked());
    assert!(clicked.row.rect.contains_rect(clicked.github.rect));
    run(vec![key(egui::Key::Enter, false)]);
    ctx.memory_mut(|memory| memory.request_focus(first.row.id));
    let clicked = run(vec![key(egui::Key::Space, true)]);
    assert!(clicked.row.clicked());
    assert!(!clicked.github.clicked());
}

#[test]
fn narrow_settings_keep_footer_visible_and_preserve_drafts() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    app.page = Page::Settings;
    let size = vec2(360., 480.);
    draw(&ctx, &mut app, size, vec![]);
    let labels = draw(&ctx, &mut app, size, vec![]);
    for title in [
        "Speichern",
        "Test-Benachrichtigung",
        "Neue Issues",
        "Konto",
        "Diagnose",
    ] {
        let rect = labels
            .iter()
            .find(|(text, _)| text == title)
            .unwrap_or_else(|| panic!("Missing {title}"))
            .1;
        assert!(
            Rect::from_min_size(pos2(0., 0.), size).contains_rect(rect),
            "{title}: {rect:?}"
        );
    }
    app.repos_text = "example/changed".into();
    app.draft.show_preview = true;
    click_text(&ctx, &mut app, size, "Konto");
    assert!(app.settings_tab == SettingsTab::Account);
    click_text(&ctx, &mut app, size, "Benachrichtigungen");
    assert!(app.settings_tab == SettingsTab::Notifications);
    assert_eq!(app.repos_text, "example/changed");
    assert!(app.draft.show_preview);
    draw(&ctx, &mut app, size, vec![escape()]);
    assert!(app.page == Page::Inbox);
    // The settings icon remains at the top right; drafts survive leaving the page.
    for pressed in [true, false] {
        let position = pos2(size.x - 26., 24.);
        draw(
            &ctx,
            &mut app,
            size,
            vec![
                egui::Event::PointerMoved(position),
                egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
    assert!(app.page == Page::Settings);
    assert_eq!(app.repos_text, "example/changed");
    click_text(&ctx, &mut app, size, "Speichern");
    assert_eq!(app.config.repositories[0].to_string(), "example/changed");
    assert!(app.config.show_preview);
    assert!(app.message.is_some());
}

#[test]
fn clicking_an_event_persists_read_state_without_opening_github() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let temporary = tempfile::tempdir().unwrap();
    let paths = Paths::at(temporary.path().join("data")).unwrap();
    let mut store = Store::open(&paths).unwrap();
    store.save_config(&app.config).unwrap();
    let event = app.events[0].clone();
    store
        .ingest("alex", std::slice::from_ref(&event), false)
        .unwrap();
    app.store = Some(store);
    app.open_url = |_| panic!("Row clicks must not open the browser");
    let size = vec2(440., 640.);
    draw(&ctx, &mut app, size, vec![]);
    click_text(&ctx, &mut app, size, &event.title);
    assert!(!app.events[0].unread);
    assert!(!Store::open(&paths).unwrap().events().unwrap()[0].unread);
    assert!(app.page == Page::Inbox);
    assert_eq!(app.visible_events().len(), 8);
    let labels = draw(&ctx, &mut app, size, vec![]);
    assert!(labels.iter().any(|(text, _)| text == &event.title));
}

#[test]
fn failed_or_unsafe_links_leave_events_unread() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    app.open_url = |_| Err(std::io::Error::other("Browser nicht verfügbar"));
    let size = vec2(360., 480.);
    let event = app.events[0].clone();
    draw(&ctx, &mut app, size, vec![]);
    click_github(&ctx, &mut app, size, &event.title);
    assert!(app.events[0].unread);
    assert!(app.message.is_some());
    app.open_url = |_| panic!("Unsafe URLs must not reach the browser");
    app.events[0].url = "https://example.com/unsafe".into();
    app.open_event(&app.events[0].clone());
    assert!(app.events[0].unread);
}

#[test]
fn mark_all_read_is_available_in_filter_menu() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let size = vec2(360., 480.);
    draw(&ctx, &mut app, size, vec![]);
    click_text(&ctx, &mut app, size, "Filter");
    click_text(&ctx, &mut app, size, "Alle als gelesen markieren");
    assert_eq!(app.visible_events().len(), 8);
    assert!(app.events.iter().all(|event| !event.unread));
    assert!(!egui::Popup::is_id_open(&ctx, filter_popup_id()));
    app.unread_only = true;
    assert!(
        draw(&ctx, &mut app, size, vec![])
            .iter()
            .any(|(text, _)| text == "Keine ungelesenen Benachrichtigungen")
    );
}

#[test]
fn repository_access_notice_opens_account_without_discarding_settings() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    app.demo = false;
    app.running = true;
    app.status.heartbeat = Utc::now().timestamp();
    app.status.warnings.push("example/private: HTTP 404".into());
    app.page = Page::Settings;
    app.repos_text = "example/unsaved".into();
    let size = vec2(640., 480.);
    draw(&ctx, &mut app, size, vec![]);
    click_text(&ctx, &mut app, size, "Zugriff prüfen");
    assert!(app.settings_tab == SettingsTab::Account);
    assert_eq!(app.repos_text, "example/unsaved");
}

#[test]
fn settings_switch_changes_only_after_click_and_supports_keyboard_focus() {
    let ctx = egui::Context::default();
    let mut value = false;
    let mut toggle = Rect::NOTHING;
    let mut run = |events| {
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_width(500.);
                let row = setting_toggle(
                    ui,
                    "Reviews",
                    "Deine Pull Requests.",
                    &mut value,
                    Palette::dark(),
                    Some(Icon::ReviewSubmitted),
                );
                toggle = row.inner.rect;
            },
        );
        output.textures_delta.clear();
        (value, toggle)
    };
    let (value, rect) = run(vec![]);
    assert!(!value);
    let pointer = |pressed| egui::Event::PointerButton {
        pos: rect.center(),
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    assert!(
        !run(vec![
            egui::Event::PointerMoved(rect.center()),
            pointer(true)
        ])
        .0
    );
    assert!(run(vec![pointer(false)]).0);
    let key = |pressed| egui::Event::Key {
        key: egui::Key::Space,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    };
    run(vec![key(true)]);
    assert!(!run(vec![key(false)]).0);
}

#[test]
fn oauth_code_is_visible_and_cancellation_discards_late_credentials() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    app.demo = false;
    app.running = true;
    app.status.heartbeat = Utc::now().timestamp();
    app.page = Page::Settings;
    app.settings_tab = SettingsTab::Account;
    app.config.login.clear();
    let (send, events) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    app.oauth_login = Some(OAuthLogin {
        events,
        cancel: cancelled.clone(),
        user_code: None,
        verification_uri: None,
        expires_at: None,
    });
    app.open_url = |url| {
        assert_eq!(url, "https://github.com/login/device");
        Ok(())
    };
    send.send(OAuthMessage::Code {
        user_code: "TEST-CODE".into(),
        verification_uri: "https://github.com/login/device".into(),
        expires_at: Instant::now() + Duration::from_secs(900),
    })
    .unwrap();
    app.poll_oauth();
    let size = vec2(360., 480.);
    draw(&ctx, &mut app, size, vec![]);
    let labels = draw(&ctx, &mut app, size, vec![]);
    for title in ["TEST-CODE", "Kopieren", "GitHub öffnen ↗", "Abbrechen"] {
        let rect = labels.iter().find(|(text, _)| text == title).unwrap().1;
        assert!(Rect::from_min_size(pos2(0., 0.), size).contains_rect(rect));
    }
    send.send(OAuthMessage::Finished(Ok(oauth::Credentials {
        access_token: "test-access".into(),
        refresh_token: Some("test-refresh".into()),
        expires_at: Some(123),
        refresh_expires_at: Some(456),
        client_id: "testclient".into(),
    })))
    .ok()
    .unwrap();
    click_text(&ctx, &mut app, size, "Abbrechen");
    assert!(cancelled.load(Ordering::Relaxed));
    app.poll_oauth();
    assert!(app.oauth_login.is_none());
    assert!(app.job.is_none());
    assert!(!app.config.oauth);
    assert!(app.config.login.is_empty());
}

#[test]
fn oauth_accounts_hide_manual_credentials_and_settings_preserve_authentication() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    app.demo = false;
    app.running = true;
    app.status.heartbeat = Utc::now().timestamp();
    app.page = Page::Settings;
    app.settings_tab = SettingsTab::Account;
    app.config.oauth = true;
    app.draft.oauth = true;
    let labels = draw(&ctx, &mut app, vec2(360., 480.), vec![]);
    assert!(!labels.iter().any(|(text, _)| text == "Manuelle Tokens"));
    let temporary = tempfile::tempdir().unwrap();
    let paths = Paths::at(temporary.path().join("data")).unwrap();
    let mut store = Store::open(&paths).unwrap();
    store.save_config(&app.config).unwrap();
    app.store = Some(store);
    app.config.oauth = false;
    app.draft.oauth = false;
    app.save_settings();
    assert!(
        app.config.oauth,
        "Preferences must not overwrite a newer authentication mode"
    );
}
