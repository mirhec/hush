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
                ("Reviews", "Deine Pull Requests.", Some(Icon::Check)),
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
fn compact_inbox_fits_eight_events_and_long_rows_do_not_grow() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let size = vec2(1040., 720.);
    draw(&ctx, &mut app, size, vec![]);
    let labels = draw(&ctx, &mut app, size, vec![]);
    for event in &app.events {
        let rect = labels
            .iter()
            .find(|(text, _)| text == &event.title)
            .expect("All eight event titles visible")
            .1;
        assert!(Rect::from_min_size(pos2(0., 0.), size).contains_rect(rect));
    }
    let mut event = app.events[0].clone();
    event.title = "Ein sehr langer Issue-Titel ohne Platz für eine weitere Zeile. ".repeat(20);
    event.repository = "organisation/".to_owned() + &"repository".repeat(30);
    for width in [280., 476., 800.] {
        let mut rect = Rect::NOTHING;
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(width);
            rect = event_card(ui, &event, false, Palette::dark()).rect;
        });
        output.textures_delta.clear();
        assert_eq!(rect.height(), 62.);
        assert!((rect.width() - width).abs() < 1.);
    }
}

#[test]
fn small_window_keeps_settings_actions_visible_and_tabs_preserve_drafts() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    app.page = Page::Settings;
    let size = vec2(640., 480.);
    draw(&ctx, &mut app, size, vec![]);
    let labels = draw(&ctx, &mut app, size, vec![]);
    for title in [
        "Speichern",
        "Test-Benachrichtigung",
        "Inhalte im Banner anzeigen",
        "Neue Issues",
        "1 Min.",
    ] {
        let rect = labels
            .iter()
            .find(|(text, _)| text == title)
            .unwrap_or_else(|| panic!("Missing {title}"))
            .1;
        assert!(
            Rect::from_min_size(pos2(56., 0.), vec2(584., 480.)).contains_rect(rect),
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
    click_text(&ctx, &mut app, size, "Speichern");
    assert_eq!(app.config.repositories[0].to_string(), "example/changed");
    assert!(app.config.show_preview);
    assert!(app.message.is_some());
}

#[test]
fn compact_list_and_details_support_select_archive_and_escape() {
    let ctx = egui::Context::default();
    let mut app = demo_app(&ctx);
    let size = vec2(640., 480.);
    let first = app.events[0].clone();
    draw(&ctx, &mut app, size, vec![]);
    click_text(&ctx, &mut app, size, &first.title);
    assert_eq!(app.selected.as_deref(), Some(first.id.as_str()));
    assert!(!app.events[0].unread);
    let labels = draw(&ctx, &mut app, size, vec![]);
    for title in ["Auf GitHub ↗", "Erledigen"] {
        let rect = labels.iter().find(|(text, _)| text == title).unwrap().1;
        assert!(Rect::from_min_size(pos2(0., 0.), size).contains_rect(rect));
    }
    click_text(&ctx, &mut app, size, "Erledigen");
    assert!(app.events[0].archived);
    assert!(app.selected.is_none());
    app.selected = Some(app.events[1].id.clone());
    draw(
        &ctx,
        &mut app,
        size,
        vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    assert!(app.selected.is_none());
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
                    Some(Icon::Check),
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
