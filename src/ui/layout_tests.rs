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
    let mut app = HushApp::new(
        &eframe::CreationContext::_new_kittest(ctx.clone()),
        None,
        None,
        Config::default(),
        true,
        false,
    );
    app.config.language = Language::De;
    app.draft.language = Language::De;
    app
}

fn finish_autostart_job(app: &mut HushApp) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while app.autostart_job.is_some() {
        app.poll_autostart();
        assert!(Instant::now() < deadline, "Autostart worker did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn autostart_only_changes_after_explicit_toggle_and_keeps_settings_drafts() {
    use std::sync::atomic::AtomicUsize;
    static READS: AtomicUsize = AtomicUsize::new(0);
    static WRITES: AtomicUsize = AtomicUsize::new(0);
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let temporary = tempfile::tempdir().unwrap();
    let paths = Paths::at(temporary.path().join("data")).unwrap();
    let mut store = Store::open(&paths).unwrap();
    store.save_config(&app.config).unwrap();
    app.store = Some(store);
    app.paths = Some(paths.clone());
    app.autostart_backend = AutostartBackend {
        status: || {
            READS.fetch_add(1, Ordering::SeqCst);
            Ok(autostart::Status {
                enabled: Some(false),
                portal: false,
            })
        },
        set_enabled: |enabled, reason| {
            assert!(reason.contains("Hush"));
            WRITES.fetch_add(1, Ordering::SeqCst);
            Ok(autostart::Status {
                enabled: Some(enabled),
                portal: false,
            })
        },
    };
    app.start_autostart_job(Some(true), &ctx);
    assert!(
        app.autostart_job.is_none(),
        "Demo never changes login settings"
    );
    assert_eq!(WRITES.load(Ordering::SeqCst), 0);
    app.demo = false;
    app.running = true;
    app.status.heartbeat = Utc::now().timestamp();
    app.page = Page::Settings;
    app.repos_text = "example/unsaved".into();
    app.draft.show_preview = true;
    let size = vec2(360., 480.);
    draw(&ctx, &mut app, size, vec![]);
    finish_autostart_job(&mut app);
    let labels = draw(&ctx, &mut app, size, vec![]);
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(WRITES.load(Ordering::SeqCst), 0);
    let title = labels
        .iter()
        .find(|(text, _)| text == "Beim Anmelden starten")
        .unwrap()
        .1;
    let position = pos2(size.x - 38., title.center().y);
    for pressed in [true, false] {
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
    assert!(app.autostart_job.is_some(), "Toggle starts a worker");
    finish_autostart_job(&mut app);
    assert_eq!(WRITES.load(Ordering::SeqCst), 1);
    assert_eq!(app.autostart_status.unwrap().enabled, Some(true));
    assert_eq!(app.repos_text, "example/unsaved");
    assert!(app.draft.show_preview);
    assert!(!Store::open(&paths).unwrap().config().unwrap().show_preview);
    draw(&ctx, &mut app, size, vec![]);
    let labels = draw(&ctx, &mut app, size, vec![]);
    assert!(
        labels
            .iter()
            .any(|(text, _)| text == "Autostart aktiviert.")
    );
    assert_eq!(WRITES.load(Ordering::SeqCst), 1);
    app.page = Page::Inbox;
    draw(&ctx, &mut app, size, vec![]);
    app.page = Page::Settings;
    draw(&ctx, &mut app, size, vec![]);
    finish_autostart_job(&mut app);
    assert_eq!(
        READS.load(Ordering::SeqCst),
        2,
        "Reopening refreshes OS state"
    );
    assert_eq!(app.autostart_status.unwrap().enabled, Some(false));
}

#[test]
fn autostart_failure_and_portal_denial_never_report_success() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    app.autostart_status = Some(autostart::Status {
        enabled: Some(true),
        portal: true,
    });
    for result in [
        Err("Permission denied".to_owned()),
        Ok(autostart::Status {
            enabled: Some(true),
            portal: true,
        }),
        Ok(autostart::Status {
            enabled: None,
            portal: true,
        }),
    ] {
        let (send, result_channel) = mpsc::channel();
        app.autostart_job = Some(AutostartJob {
            requested: Some(false),
            result: result_channel,
        });
        app.poll_autostart();
        assert!(app.autostart_job.is_some(), "Pending request stays pending");
        send.send(result.clone()).unwrap();
        app.poll_autostart();
        assert!(app.autostart_job.is_none());
        assert!(app.autostart_error.is_some());
        if result.is_err() {
            assert_eq!(app.autostart_status.unwrap().enabled, Some(true));
        }
        let labels = draw(&ctx, &mut app, vec2(360., 480.), vec![]);
        assert!(
            !labels
                .iter()
                .any(|(text, _)| text == "Autostart deaktiviert.")
        );
    }
    let (send, result) = mpsc::channel();
    app.autostart_job = Some(AutostartJob {
        requested: Some(false),
        result,
    });
    drop(send);
    app.poll_autostart();
    assert!(app.autostart_job.is_none());
    assert_eq!(
        app.autostart_error.as_deref(),
        Some("Die Autostart-Aktion wurde abgebrochen. Bitte erneut versuchen.")
    );
}

#[test]
fn portal_autostart_actions_fit_all_languages_without_assuming_off() {
    for language in Language::ALL
        .into_iter()
        .filter(|language| *language != Language::System)
    {
        let ctx = egui::Context::default();
        let mut app = demo_app(&ctx);
        app.config.language = language;
        theme::fonts(&ctx, language);
        app.page = Page::Settings;
        app.autostart_status = Some(autostart::Status {
            enabled: None,
            portal: true,
        });
        let size = vec2(360., 480.);
        draw(&ctx, &mut app, size, vec![]);
        let labels = draw(&ctx, &mut app, size, vec![]);
        for title in [
            "Beim Anmelden starten",
            "Systemdialog",
            "Autostart aktivieren",
            "Autostart deaktivieren",
            "Speichern",
            "Test-Benachrichtigung",
        ] {
            let rect = labels
                .iter()
                .find(|(text, _)| text == language.text(title))
                .unwrap_or_else(|| panic!("{language:?}: missing {title}"))
                .1;
            assert!(
                Rect::from_min_size(pos2(0., 0.), size).contains_rect(rect),
                "{language:?}: {title}: {rect:?}"
            );
        }
        assert_eq!(app.autostart_status.unwrap().enabled, None);
        assert!(
            app.autostart_job.is_none(),
            "Rendering does not prompt for a permission"
        );
    }
}

#[test]
fn portal_autostart_buttons_wait_for_confirmation_and_prevent_duplicate_requests() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let temporary = tempfile::tempdir().unwrap();
    app.paths = Some(Paths::at(temporary.path().join("data")).unwrap());
    app.demo = false;
    app.running = true;
    app.status.heartbeat = Utc::now().timestamp();
    app.page = Page::Settings;
    app.autostart_status = Some(autostart::Status {
        enabled: None,
        portal: true,
    });
    app.autostart_checked = true;
    app.autostart_backend.set_enabled = |enabled, _| {
        Ok(autostart::Status {
            enabled: Some(enabled),
            portal: true,
        })
    };
    let size = vec2(360., 480.);
    draw(&ctx, &mut app, size, vec![]);
    click_text(&ctx, &mut app, size, "Autostart aktivieren");
    assert_eq!(
        app.autostart_status.unwrap().enabled,
        None,
        "A click does not imply consent"
    );
    assert_eq!(app.autostart_job.as_ref().unwrap().requested, Some(true));
    let labels = draw(&ctx, &mut app, size, vec![]);
    assert!(
        labels
            .iter()
            .any(|(text, _)| text == "Autostart wird geändert …")
    );
    click_text(&ctx, &mut app, size, "Autostart deaktivieren");
    assert_eq!(
        app.autostart_job.as_ref().unwrap().requested,
        Some(true),
        "Pending actions stay disabled"
    );
    finish_autostart_job(&mut app);
    assert_eq!(app.autostart_status.unwrap().enabled, Some(true));
    click_text(&ctx, &mut app, size, "Autostart deaktivieren");
    finish_autostart_job(&mut app);
    assert_eq!(app.autostart_status.unwrap().enabled, Some(false));
}

#[test]
fn language_and_theme_changes_persist_without_saving_other_drafts() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let temporary = tempfile::tempdir().unwrap();
    let paths = Paths::at(temporary.path().join("data")).unwrap();
    let mut store = Store::open(&paths).unwrap();
    store.save_config(&app.config).unwrap();
    app.store = Some(store);
    app.page = Page::Settings;
    app.repos_text = "example/unsaved".into();
    app.draft.show_preview = true;
    let size = vec2(360., 480.);
    draw(&ctx, &mut app, size, vec![]);
    click_text(&ctx, &mut app, size, "Deutsch");
    click_text(&ctx, &mut app, size, "日本語");
    assert_eq!(app.language(), Language::Ja);
    assert_eq!(
        Store::open(&paths).unwrap().config().unwrap().language,
        Language::Ja
    );
    assert_eq!(app.repos_text, "example/unsaved");
    assert!(app.draft.show_preview);
    assert!(!Store::open(&paths).unwrap().config().unwrap().show_preview);
    app.save_appearance(Language::En, true, &ctx);
    let saved = Store::open(&paths).unwrap().config().unwrap();
    assert_eq!(saved.language, Language::En);
    assert!(saved.light_theme && app.light);
    assert_eq!(saved.login, "alex");
    let labels = draw(&ctx, &mut app, size, vec![]);
    assert!(labels.iter().any(|(text, _)| text == "Settings"));
    assert!(labels.iter().any(|(text, _)| text == "Save"));
}

#[test]
fn all_languages_fit_narrow_settings_and_preserve_github_content() {
    for language in Language::ALL
        .into_iter()
        .filter(|language| *language != Language::System)
    {
        let ctx = egui::Context::default();
        let mut app = demo_app(&ctx);
        app.config.language = language;
        app.draft.language = language;
        theme::fonts(&ctx, language);
        let size = vec2(360., 480.);
        app.events[0].title = "Einstellungen gespeichert. {count}".into();
        draw(&ctx, &mut app, size, vec![]);
        let labels = draw(&ctx, &mut app, size, vec![]);
        assert!(
            labels
                .iter()
                .any(|(text, _)| text == language.text("Posteingang"))
        );
        assert!(
            labels
                .iter()
                .any(|(text, _)| text == "Einstellungen gespeichert. {count}")
        );
        app.page = Page::Settings;
        for tab in [
            SettingsTab::Notifications,
            SettingsTab::Account,
            SettingsTab::Diagnostics,
        ] {
            app.settings_tab = tab;
            draw(&ctx, &mut app, size, vec![]);
            let labels = draw(&ctx, &mut app, size, vec![]);
            for title in ["Sprache", "Benachrichtigungen", "Konto", "Diagnose"] {
                let rect = labels
                    .iter()
                    .find(|(text, _)| text == language.text(title))
                    .unwrap_or_else(|| panic!("{language:?}: missing {title}"))
                    .1;
                assert!(
                    Rect::from_min_size(pos2(0., 0.), size).contains_rect(rect),
                    "{language:?}: {title} is outside the window: {rect:?}"
                );
            }
            if tab != SettingsTab::Diagnostics {
                let rect = labels
                    .iter()
                    .find(|(text, _)| text == language.text("Speichern"))
                    .unwrap()
                    .1;
                assert!(Rect::from_min_size(pos2(0., 0.), size).contains_rect(rect));
            }
        }
        ctx.fonts_mut(|fonts| {
            let font = FontId::proportional(13.);
            assert!(fonts.has_glyphs(&font, "简体中文日本語设置通知設定確認審査"));
            assert!(fonts.has_glyphs(&font, language.text("Benachrichtigungen")));
        });
    }
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
            rect = event_card(ui, &event, Palette::dark(), Language::De)
                .row
                .rect;
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
                responses = Some(event_card(ui, &event, Palette::dark(), Language::De));
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
fn unread_action_is_keyboard_reachable_and_separate_from_row_and_github() {
    for width in [280., 336., 416.] {
        let ctx = egui::Context::default();
        let mut event = demo::events().remove(0);
        event.unread = false;
        let mut responses = None;
        let mut run = |events| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(width);
                    responses = Some(event_card(ui, &event, Palette::dark(), Language::De));
                },
            );
            output.textures_delta.clear();
            responses.take().unwrap()
        };
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let first = run(vec![]);
        let unread = first.unread.as_ref().unwrap();
        assert!(first.row.rect.contains_rect(unread.rect));
        assert!(first.row.rect.contains_rect(first.github.rect));
        assert!(!unread.rect.intersects(first.github.rect));
        assert_eq!(first.row.rect.height(), 56.);
        ctx.memory_mut(|memory| memory.request_focus(first.row.id));
        run(vec![key(egui::Key::Tab, true)]);
        let focused = run(vec![key(egui::Key::Tab, false)]);
        assert!(focused.unread.as_ref().unwrap().has_focus());
        run(vec![key(egui::Key::Tab, true)]);
        let focused = run(vec![key(egui::Key::Tab, false)]);
        assert!(focused.github.has_focus());
        for activation in [egui::Key::Enter, egui::Key::Space] {
            ctx.memory_mut(|memory| memory.request_focus(unread.id));
            let clicked = run(vec![key(activation, true)]);
            assert!(clicked.unread.unwrap().clicked());
            assert!(!clicked.row.clicked());
            assert!(!clicked.github.clicked());
            assert!(ctx.memory(|memory| memory.has_focus(first.row.id)));
            run(vec![key(activation, false)]);
        }
    }
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
fn unread_button_restores_persisted_read_event_without_opening_github() {
    for width in [360., 440.] {
        let ctx = egui::Context::default();
        let mut app = demo_app(&ctx);
        let temporary = tempfile::tempdir().unwrap();
        let paths = Paths::at(temporary.path().join("data")).unwrap();
        let mut store = Store::open(&paths).unwrap();
        store.save_config(&app.config).unwrap();
        let event = app.events[0].clone();
        store
            .ingest("alex", std::slice::from_ref(&event), true)
            .unwrap();
        app.store = Some(store);
        app.open_url = |_| panic!("Marking unread must not open the browser");
        let size = vec2(width, 480.);
        draw(&ctx, &mut app, size, vec![]);
        click_text(&ctx, &mut app, size, &event.title);
        assert!(!app.events[0].unread);
        let labels = draw(&ctx, &mut app, size, vec![]);
        let title_rect = labels
            .iter()
            .find(|(text, _)| text == &event.title)
            .unwrap()
            .1;
        let position = pos2(size.x - 58., title_rect.top() + 10.);
        draw(
            &ctx,
            &mut app,
            size,
            vec![egui::Event::PointerMoved(position)],
        );
        for pressed in [true, false] {
            draw(
                &ctx,
                &mut app,
                size,
                vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
        assert!(app.events[0].unread);
        assert!(Store::open(&paths).unwrap().events().unwrap()[0].unread);
        assert!(app.store.as_ref().unwrap().outbox().unwrap().is_empty());
        assert_eq!(app.visible_events().len(), 8);
        app.unread_only = true;
        assert!(app.visible_events().iter().any(|item| item.id == event.id));
        app.unread_only = false;
        click_text(&ctx, &mut app, size, &event.title);
        assert!(!app.events[0].unread);
        assert!(!Store::open(&paths).unwrap().events().unwrap()[0].unread);
    }
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
