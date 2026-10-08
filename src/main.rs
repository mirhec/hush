#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
use anyhow::{Result, bail};
use hush::{
    engine,
    model::Config,
    storage::{Paths, Store},
};

fn main() {
    if let Err(error) = run() {
        eprintln!("Hush: {error:#}");
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() > 1 {
        bail!("Nur eine Option gleichzeitig. Hilfe: hush --help");
    }
    let option = args.first().map(String::as_str).unwrap_or("");
    if matches!(option, "--help" | "-h") {
        println!(
            "Hush — GitHub-Benachrichtigungen\n\n  hush                     Fenster öffnen; Hintergrunddienst starten\n  hush --demo              Offline-Demo, ohne Datenbank und Zugangsdaten\n  hush --tray              Im Tray starten, ohne Fenster\n  hush --start             Hintergrunddienst starten und Start prüfen\n  hush --background        Nur Hintergrunddienst, höchstens eine Instanz\n  hush --status            Tokenfreier JSON-Status für DMS / Skripte\n  hush --stop              Hintergrunddienst beenden\n  hush --test-notification  Native Test-Benachrichtigung senden\n\nZugangsdaten nur in der Oberfläche eingeben, niemals als Argument."
        );
        return Ok(());
    }
    if option == "--version" {
        println!("Hush {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    // Demo bypasses path discovery, keyring, SQLite, background spawning and HTTP entirely.
    if option == "--demo" {
        return desktop(None, None, Config::default(), true, false);
    }
    if ![
        "",
        "--tray",
        "--window",
        "--start",
        "--background",
        "--status",
        "--stop",
        "--test-notification",
    ]
    .contains(&option)
    {
        bail!("Unbekannte Option. Hilfe: hush --help");
    }
    let paths = Paths::discover()?;
    match option {
        "--background" => engine::run_background(paths),
        "--start" => engine::launch(&paths),
        "--status" => {
            let s = Store::open(&paths)?;
            let status = s.status()?;
            let unread = s
                .events()?
                .iter()
                .filter(|e| e.unread && !e.archived)
                .count();
            let cfg = s.config()?;
            let running = engine::is_running(&paths)?;
            println!(
                "{}",
                serde_json::json!({"app":"hush","unread":unread,"running":running,"healthy":running&&status.alive(),"phase":status.phase,"service_error":status.service_error,"configured":!cfg.login.is_empty(),"paused":cfg.paused(),"last_sync":status.last_sync,"next_sync":status.next_sync,"warnings":status.warnings.len(),"notification_error":status.notification_error.is_some()})
            );
            Ok(())
        }
        "--stop" => Store::open(&paths)?.request_stop(),
        "--test-notification" => hush::notify::test(),
        #[cfg(all(feature = "desktop", target_os = "linux"))]
        "" | "--tray" => hush::linux_desktop::run(paths, option == "--tray"),
        _ => {
            let store = Store::open(&paths)?;
            let config = store.config()?;
            desktop(Some(paths), Some(store), config, false, option == "--tray")
        }
    }
}
#[cfg(feature = "desktop")]
fn desktop(
    paths: Option<Paths>,
    store: Option<Store>,
    config: Config,
    demo: bool,
    start_hidden: bool,
) -> Result<()> {
    use eframe::egui;
    use fs2::FileExt;
    // Keep the lock alive until eframe exits. A second launcher restores the
    // existing window instead of creating another icon or hidden instance.
    let _window_lock = if let Some(paths) = &paths {
        let file = paths.window_lock_file()?;
        match file.try_lock_exclusive() {
            Ok(()) => Some(file),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if !start_hidden {
                    store.as_ref().unwrap().put("show_window", "1")?;
                }
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        }
    } else {
        None
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_app_id(hush::model::APP_ID)
            .with_title(if demo { "Hush · Offline-Demo" } else { "Hush" })
            .with_inner_size([1240., 860.])
            .with_min_inner_size([900., 700.])
            .with_visible(!start_hidden)
            .with_icon(hush::tray::icon()),
        renderer: eframe::Renderer::Glow,
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Hush",
        options,
        Box::new(move |cc| {
            Ok(Box::new(hush::ui::HushApp::new(
                cc,
                paths,
                store,
                config,
                demo,
                start_hidden,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("Fenster konnte nicht geöffnet werden: {e}"))
}
#[cfg(not(feature = "desktop"))]
fn desktop(_: Option<Paths>, _: Option<Store>, _: Config, _: bool, _: bool) -> Result<()> {
    bail!("Ohne desktop-Feature gebaut. Für das Fenster den Standard-Build verwenden.")
}
