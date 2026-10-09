//! Export the actual egui meshes and font atlas for visual review without a display server.
use super::*;
use eframe::App;
use serde_json::json;

#[test]
#[ignore = "Set HUSH_UI_CAPTURE_DIR to export offline demo frames for scripts/render-ui.html"]
fn export_native_ui_frames() {
    let directory = std::path::PathBuf::from(std::env::var("HUSH_UI_CAPTURE_DIR").unwrap());
    std::fs::create_dir_all(&directory).unwrap();
    let cases = vec![
        ("inbox", 440., 640., Page::Inbox, false, false),
        ("inbox-all", 440., 640., Page::Inbox, false, false),
        ("inbox-hover", 440., 640., Page::Inbox, false, false),
        ("inbox-small-hover", 360., 480., Page::Inbox, false, false),
        (
            "inbox-unread-action-hover",
            440.,
            640.,
            Page::Inbox,
            false,
            false,
        ),
        (
            "inbox-unread-action-small-hover",
            360.,
            480.,
            Page::Inbox,
            false,
            false,
        ),
        (
            "inbox-unread-action-light-hover",
            360.,
            480.,
            Page::Inbox,
            false,
            true,
        ),
        ("filters", 440., 640., Page::Inbox, true, false),
        ("filters-small", 360., 480., Page::Inbox, true, false),
        ("settings", 440., 640., Page::Settings, false, false),
        ("inbox-small", 360., 480., Page::Inbox, false, false),
        ("settings-small", 360., 480., Page::Settings, false, false),
        ("inbox-light", 440., 640., Page::Inbox, false, true),
        ("account", 440., 640., Page::Settings, false, false),
        ("account-oauth", 360., 480., Page::Settings, false, false),
        ("account-small", 360., 480., Page::Settings, false, false),
        ("diagnostics", 360., 480., Page::Settings, false, false),
    ];
    let mut cases: Vec<_> = cases
        .into_iter()
        .map(|(name, width, height, page, filters, light)| {
            (name.to_owned(), width, height, page, filters, light)
        })
        .collect();
    for locale in ["en", "de", "es", "fr", "pt", "zh", "ja"] {
        for (screen, page) in [
            ("inbox", Page::Inbox),
            ("settings", Page::Settings),
            ("account", Page::Settings),
            ("empty", Page::Inbox),
        ] {
            let name = format!("{locale}-{screen}");
            cases.push((name, 360., 480., page, false, false));
        }
    }
    for (name, width, height, page, filters, light) in cases {
        let ctx = egui::Context::default();
        let mut app = HushApp::new(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            None,
            None,
            Config::default(),
            true,
            false,
        );
        app.page = page;
        let language = name
            .split_once('-')
            .map(|(code, _)| Language::from_locale(code))
            .unwrap_or(Language::De);
        let language = if name.starts_with("en-")
            || name.starts_with("de-")
            || name.starts_with("es-")
            || name.starts_with("fr-")
            || name.starts_with("pt-")
            || name.starts_with("zh-")
            || name.starts_with("ja-")
        {
            language
        } else {
            Language::De
        };
        app.config.language = language;
        app.draft.language = language;
        theme::fonts(&ctx, language);
        app.light = light;
        theme::apply(&ctx, light);
        // Capture settled popup opacity, independent of rendering speed.
        ctx.global_style_mut(|style| style.animation_time = 0.);
        if name.starts_with("account") || name.ends_with("-account") {
            app.settings_tab = SettingsTab::Account;
            app.demo = false;
            app.running = true;
            app.status.heartbeat = Utc::now().timestamp();
            app.status.last_sync = Some(Utc::now().timestamp());
            app.config.login.clear();
            app.draft = app.config.clone();
        }
        if name == "account-oauth" || name.ends_with("-account") {
            let (_send, events) = mpsc::channel();
            app.oauth_login = Some(OAuthLogin {
                events,
                cancel: Arc::new(AtomicBool::new(false)),
                user_code: Some("TEST-CODE".into()),
                verification_uri: Some("https://github.com/login/device".into()),
                expires_at: Some(Instant::now() + Duration::from_secs(900)),
            });
        }
        if name == "diagnostics" {
            app.settings_tab = SettingsTab::Diagnostics;
        }
        if name == "inbox-all" {
            app.unread_only = false;
        }
        if name.contains("unread-action") {
            app.events[0].unread = false;
        }
        if name.ends_with("-empty") {
            app.events.clear();
        }
        if filters {
            egui::Popup::open_id(&ctx, filter_popup_id());
        }
        let mut textures = Vec::new();
        let mut meshes = Vec::new();
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0., 0.), vec2(width, height))),
                    events: if name.ends_with("hover") {
                        let inset = if name.contains("unread-action") {
                            58.
                        } else {
                            26.
                        };
                        vec![egui::Event::PointerMoved(pos2(width - inset, 110.))]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ui| app.ui(ui, &mut eframe::Frame::_new_kittest()),
            );
            for (id, deltas) in &output.textures_delta.set {
                for delta in deltas {
                    let egui::ImageData::Color(image) = &delta.image;
                    textures.push(json!({
                        "id": format!("{id:?}"), "size": image.size, "pos": delta.pos,
                        "pixels": image.pixels.iter().flat_map(|p| p.to_array()).collect::<Vec<_>>()
                    }));
                }
            }
            output.textures_delta.clear();
            meshes = ctx.tessellate(output.shapes, output.pixels_per_point).into_iter().filter_map(|primitive| {
                let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive else { return None };
                Some(json!({
                    "clip": [primitive.clip_rect.left(), primitive.clip_rect.top(), primitive.clip_rect.right(), primitive.clip_rect.bottom()],
                    "texture": format!("{:?}", mesh.texture_id),
                    "vertices": mesh.vertices.iter().map(|v| {
                        let c = v.color.to_array();
                        [v.pos.x, v.pos.y, v.uv.x, v.uv.y, c[0] as f32 / 255., c[1] as f32 / 255., c[2] as f32 / 255., c[3] as f32 / 255.]
                    }).collect::<Vec<_>>(),
                    "indices": mesh.indices
                }))
            }).collect();
        }
        std::fs::write(
            directory.join(format!("{name}.json")),
            serde_json::to_vec(
                &json!({"width": width, "height": height, "textures": textures, "meshes": meshes}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}
