//! Linux owns the tray independently of the egui window. Wayland/winit cannot
//! hide an existing window: closing must destroy it, reopening creates a new one.
use crate::{
    engine,
    storage::{Paths, Store},
    tray::{Event, State, Tray},
};
use anyhow::{Context, Result};
use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

const NO_TRAY: &str = "Keine Tray-Leiste verfügbar. In Niri/DMS das System-Tray aktivieren.";

#[derive(Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopStatus {
    pub tray_available: bool,
    pub error: Option<String>,
}
pub fn status(store: &Store) -> Result<DesktopStatus> {
    Ok(store
        .get("desktop_status")?
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}
fn save_status(store: &Store, status: &DesktopStatus) -> Result<()> {
    store.put("desktop_status", &serde_json::to_string(status)?)
}
fn try_lock(file: File) -> Result<Option<File>> {
    match file.try_lock_exclusive() {
        Ok(()) => Ok(Some(file)),
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn window_running(paths: &Paths) -> Result<bool> {
    Ok(try_lock(paths.window_lock_file()?)?.is_none())
}

#[derive(Default)]
struct Window {
    child: Option<Child>,
}
impl Window {
    fn reap(&mut self) -> Result<Option<std::process::ExitStatus>> {
        if let Some(child) = &mut self.child
            && let Some(status) = child.try_wait()?
        {
            self.child = None;
            return Ok(Some(status));
        }
        Ok(None)
    }
    fn open(&mut self, paths: &Paths, store: &Store, settings: bool) -> Result<()> {
        self.reap()?;
        if settings {
            store.put("show_settings", "1")?;
        }
        // Also covers a child that is starting but has not acquired its lock yet.
        if self.child.is_some() || window_running(paths)? {
            store.put("show_window", "1")?;
            return Ok(());
        }
        let log = paths.window_log()?;
        self.child = Some(
            Command::new(std::env::current_exe()?)
                .arg("--window")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::from(log))
                .spawn()
                .context("Hush-Fenster konnte nicht gestartet werden.")?,
        );
        Ok(())
    }
}

pub fn run(paths: Paths, start_in_tray: bool) -> Result<()> {
    let mut store = Store::open(&paths)?;
    let Some(_lock) = try_lock(paths.tray_lock_file()?)? else {
        if !start_in_tray {
            store.put("open_window", "1")?;
        }
        return Ok(());
    };
    for flag in ["quit_desktop", "quit_window", "show_settings"] {
        store.take_flag(flag)?;
    }
    let mut desktop = DesktopStatus::default();
    save_status(&store, &desktop)?;
    let mut tray = Tray::without_window(paths.root.join("tray"))?;
    let mut window = Window::default();
    let mut initialized = false;
    let mut quitting = false;
    if let Err(error) = engine::launch(&paths) {
        desktop.error = Some(format!("{error:#}"));
        save_status(&store, &desktop)?;
    }
    if !start_in_tray || desktop.error.is_some() {
        window.open(&paths, &store, false)?;
    }
    loop {
        if let Some(exit) = window.reap()? {
            if !exit.success() {
                desktop.error = Some(format!(
                    "Fenster wurde beendet ({exit}). Details: {}",
                    paths.root.join("window.log").display()
                ));
                save_status(&store, &desktop)?;
                eprintln!("Hush: {}", desktop.error.as_deref().unwrap_or_default());
            }
            if initialized && !desktop.tray_available && !quitting {
                anyhow::ensure!(
                    exit.success(),
                    "Fenster konnte nicht geöffnet werden. Details: {}",
                    paths.root.join("window.log").display()
                );
                return Ok(());
            }
        }
        if store.take_flag("quit_desktop")? {
            quitting = true;
        }
        let events: Vec<_> = tray.events.try_iter().collect();
        let mut open = store.take_flag("open_window")?;
        let mut settings = false;
        for event in events {
            if quitting {
                continue;
            }
            let result = match event {
                Event::Available(available) => {
                    initialized = true;
                    desktop.tray_available = available;
                    if !available {
                        desktop.error = Some(NO_TRAY.into());
                        open = true;
                    } else if desktop.error.as_deref() == Some(NO_TRAY) {
                        desktop.error = None;
                    }
                    save_status(&store, &desktop)
                }
                Event::Error(error) => {
                    initialized = true;
                    desktop.tray_available = false;
                    desktop.error = Some(error);
                    open = true;
                    save_status(&store, &desktop)
                }
                Event::Open => {
                    open = true;
                    Ok(())
                }
                Event::Settings => {
                    open = true;
                    settings = true;
                    Ok(())
                }
                Event::Refresh => store.request_refresh(),
                Event::Pause => {
                    let mut config = store.config()?;
                    config.paused_until = if config.paused() {
                        0
                    } else {
                        Utc::now().timestamp() + 1800
                    };
                    store.save_config(&config).map(|_| ())
                }
                Event::Start => engine::launch(&paths),
                Event::Quit => {
                    quitting = true;
                    Ok(())
                }
            };
            if let Err(error) = result {
                desktop.error = Some(format!("{error:#}"));
                save_status(&store, &desktop)?;
                open = true;
            }
        }
        if quitting {
            // Repeat while an in-flight GUI account/start action finishes. A
            // fresh worker clears stale shutdown flags during initialization.
            store.request_stop()?;
            store.put("quit_window", "1")?;
            if !engine::is_running(&paths)? && window.child.is_none() && !window_running(&paths)? {
                save_status(&store, &DesktopStatus::default())?;
                return Ok(());
            }
        } else if open {
            window.open(&paths, &store, settings)?;
        }
        if initialized
            && !desktop.tray_available
            && !quitting
            && window.child.is_none()
            && !window_running(&paths)?
        {
            return Ok(());
        }
        let config = store.config()?;
        let runtime = store.status()?;
        tray.update(State {
            running: engine::is_running(&paths)? && runtime.alive(),
            paused: config.paused(),
            unread: store
                .events()?
                .iter()
                .filter(|e| e.unread)
                .count(),
            warning: runtime.notification_error.is_some()
                || !runtime.warnings.is_empty()
                || runtime.service_error.is_some()
                || desktop.error.is_some()
                || config.login.is_empty(),
        });
        thread::sleep(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_launcher_reuses_tray_and_requests_existing_window() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut store = Store::open(&paths)?;
        let _owner = try_lock(paths.tray_lock_file()?)?.unwrap();
        run(paths.clone(), false)?;
        assert!(store.take_flag("open_window")?);
        run(paths, true)?;
        assert!(!store.take_flag("open_window")?);
        Ok(())
    }

    #[test]
    fn tray_open_targets_existing_window_without_spawning_another() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut store = Store::open(&paths)?;
        let _window_lock = try_lock(paths.window_lock_file()?)?.unwrap();
        let mut window = Window::default();
        window.open(&paths, &store, true)?;
        assert!(window.child.is_none());
        assert!(store.take_flag("show_window")?);
        assert!(store.take_flag("show_settings")?);
        Ok(())
    }
}
