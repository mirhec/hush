use super::*;

#[test]
fn settings_rows_keep_equal_width_and_align_switches_with_wrapped_text() {
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
                (card.right() - toggle.right() - 15.).abs() < 1.,
                "right padding"
            );
        }
        assert!(rows[1].0.height() >= rows[0].0.height());
        assert!(rows[0].0.bottom() <= rows[1].0.top());
    }
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
