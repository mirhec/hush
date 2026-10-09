//! Native tray. Linux uses StatusNotifier over D-Bus; Windows/macOS use
//! eframe's native event loop. All callbacks wake App::logic, even when hidden.
#[cfg(target_os = "linux")]
mod linux;
use crate::i18n::Language;
use anyhow::Result;
use eframe::egui;
use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
};
#[cfg(not(target_os = "linux"))]
use tray_icon::{
    TrayIcon, TrayIconBuilder,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

#[derive(Debug)]
pub enum Event {
    Available(bool),
    Error(String),
    Open,
    Settings,
    Refresh,
    Pause,
    Start,
    Quit,
}

#[derive(Clone, PartialEq, Eq)]
pub struct State {
    pub language: Language,
    pub running: bool,
    pub paused: bool,
    pub unread: usize,
    pub warning: bool,
}

impl State {
    fn title(&self) -> String {
        let phase = if !self.running {
            "Dienst inaktiv"
        } else if self.paused {
            "Pausiert"
        } else if self.warning {
            "Verbindung prüfen"
        } else {
            "Aktiv"
        };
        self.language.format(
            "Hush · {phase} · {} ungelesen",
            &[
                ("phase", self.language.text(phase)),
                ("0", &self.unread.to_string()),
            ],
        )
    }
}

#[derive(Clone)]
struct Events {
    tx: Sender<Event>,
    ctx: Option<egui::Context>,
}
impl Events {
    fn send(&self, event: Event) {
        let _ = self.tx.send(event);
        if let Some(ctx) = &self.ctx {
            ctx.request_repaint();
        }
    }
}

#[cfg(not(target_os = "linux"))]
struct NativeTray {
    icon: TrayIcon,
    status: MenuItem,
    open: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
    pause: MenuItem,
    start: MenuItem,
    refresh: MenuItem,
}
#[cfg(not(target_os = "linux"))]
impl NativeTray {
    fn new(events: Events, directory: PathBuf) -> Result<Self> {
        let language = Language::System.resolved();
        let menu = Menu::new();
        let status = MenuItem::new(language.text("Hush · Dienst startet …"), false, None);
        let open = MenuItem::with_id("open", language.text("Hush öffnen"), true, None);
        let settings = MenuItem::with_id("settings", language.text("Einstellungen"), true, None);
        let refresh =
            MenuItem::with_id("refresh", language.text("Jetzt aktualisieren"), false, None);
        let pause = MenuItem::with_id("pause", language.text("30 Minuten pausieren"), true, None);
        let start = MenuItem::with_id("start", language.text("Dienst starten"), true, None);
        let quit = MenuItem::with_id("quit", language.text("Hush beenden"), true, None);
        menu.append_items(&[
            &status,
            &PredefinedMenuItem::separator(),
            &open,
            &settings,
            &refresh,
            &pause,
            &start,
            &PredefinedMenuItem::separator(),
            &quit,
        ])?;
        let menu_events = events.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let event = match event.id.as_ref() {
                "open" => Event::Open,
                "settings" => Event::Settings,
                "refresh" => Event::Refresh,
                "pause" => Event::Pause,
                "start" => Event::Start,
                "quit" => Event::Quit,
                _ => return,
            };
            menu_events.send(event);
        }));
        // A primary click opens the window; the secondary click keeps the menu.
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            if matches!(
                event,
                tray_icon::TrayIconEvent::Click {
                    button: tray_icon::MouseButton::Left,
                    button_state: tray_icon::MouseButtonState::Up,
                    ..
                }
            ) {
                events.send(Event::Open);
            }
        }));
        let data = icon();
        let builder = TrayIconBuilder::new()
            .with_id("io.hush.github")
            .with_menu(Box::new(menu))
            .with_tooltip(language.text("Hush · GitHub Inbox"))
            .with_temp_dir_path(directory)
            .with_icon(tray_icon::Icon::from_rgba(
                data.rgba,
                data.width,
                data.height,
            )?);
        let builder = builder.with_menu_on_left_click(false);
        let icon = builder.build()?;
        Ok(Self {
            icon,
            status,
            open,
            settings,
            quit,
            pause,
            start,
            refresh,
        })
    }
    fn update(&self, state: &State, icon_changed: bool) -> Result<()> {
        if icon_changed {
            let data = icon_with_unread(state.unread > 0);
            self.icon.set_icon(Some(tray_icon::Icon::from_rgba(
                data.rgba,
                data.width,
                data.height,
            )?))?;
        }
        let language = state.language;
        let text = state.title();
        self.open.set_text(language.text("Hush öffnen"));
        self.settings.set_text(language.text("Einstellungen"));
        self.quit.set_text(language.text("Hush beenden"));
        self.start.set_text(language.text("Dienst starten"));
        self.refresh.set_text(language.text("Jetzt aktualisieren"));
        self.status.set_text(&text);
        let _ = self.icon.set_tooltip(Some(&text));
        self.pause.set_text(language.text(if state.paused {
            "Benachrichtigungen fortsetzen"
        } else {
            "30 Minuten pausieren"
        }));
        self.start.set_enabled(!state.running);
        self.refresh.set_enabled(state.running);
        Ok(())
    }
}

pub struct Tray {
    pub events: Receiver<Event>,
    #[cfg(target_os = "linux")]
    commands: Sender<Option<State>>,
    #[cfg(not(target_os = "linux"))]
    native: NativeTray,
    #[cfg(not(target_os = "linux"))]
    sink: Events,
    last_state: Option<State>,
}
impl Tray {
    pub fn new(ctx: &egui::Context, directory: PathBuf) -> Result<Self> {
        Self::create(Some(ctx.clone()), directory)
    }
    #[cfg(target_os = "linux")]
    pub fn without_window(directory: PathBuf) -> Result<Self> {
        Self::create(None, directory)
    }
    fn create(ctx: Option<egui::Context>, directory: PathBuf) -> Result<Self> {
        let (tx, events) = mpsc::channel();
        let sink = Events { tx, ctx };
        #[cfg(target_os = "linux")]
        {
            let (commands, updates) = mpsc::channel();
            let _ = directory;
            std::thread::spawn(move || {
                if let Err(error) = linux::run(sink.clone(), updates) {
                    sink.send(Event::Error(format!(
                        "Tray konnte nicht gestartet werden: {error:#}"
                    )));
                }
            });
            Ok(Self {
                events,
                commands,
                last_state: None,
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let native = NativeTray::new(sink.clone(), directory)?;
            sink.send(Event::Available(true));
            Ok(Self {
                events,
                native,
                sink,
                last_state: None,
            })
        }
    }
    pub fn update(&mut self, state: State) {
        if self.last_state.as_ref() == Some(&state) {
            return;
        }
        #[cfg(target_os = "linux")]
        let _ = self.commands.send(Some(state.clone()));
        #[cfg(not(target_os = "linux"))]
        {
            let had_unread = self
                .last_state
                .as_ref()
                .is_some_and(|state| state.unread > 0);
            if let Err(error) = self.native.update(&state, had_unread != (state.unread > 0)) {
                self.sink.send(Event::Error(format!("{error:#}")));
                return;
            }
        }
        self.last_state = Some(state);
    }
}
#[cfg(target_os = "linux")]
impl Drop for Tray {
    fn drop(&mut self) {
        let _ = self.commands.send(None);
    }
}

pub fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    ctx.request_repaint();
}

pub fn icon() -> egui::IconData {
    let size = 64;
    let mut rgba = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let inside = (8..56).contains(&x) && (8..56).contains(&y);
            let stroke = [(21, 21, 45), (31, 29, 45), (41, 25, 45)]
                .iter()
                .any(|&(cx, top, bottom)| x >= cx - 2 && x <= cx + 2 && y >= top && y <= bottom);
            let color = if stroke {
                [171, 228, 194, 255]
            } else if inside {
                [23, 43, 31, 255]
            } else {
                [0, 0, 0, 0]
            };
            rgba[(y * size + x) * 4..(y * size + x + 1) * 4].copy_from_slice(&color);
        }
    }
    egui::IconData {
        rgba,
        width: size as u32,
        height: size as u32,
    }
}

/// Composite the unread indicator into the tray pixels so every host can show it.
/// The application/window icon stays unchanged, and the exact count is in the tooltip.
pub(super) fn icon_with_unread(unread: bool) -> egui::IconData {
    let mut data = icon();
    if !unread {
        return data;
    }
    for y in 0..data.height {
        for x in 0..data.width {
            let distance =
                ((x as f32 + 0.5 - 50.0).powi(2) + (y as f32 + 0.5 - 14.0).powi(2)).sqrt();
            let offset = ((y * data.width + x) * 4) as usize;
            let pixel = &mut data.rgba[offset..offset + 4];
            // The pale rim keeps the dot visible on both dark and light panels.
            for (radius, color) in [(12.0, [242, 245, 242]), (9.5, [240, 76, 91])] {
                let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
                if coverage == 0.0 {
                    continue;
                }
                let background = pixel[3] as f32 / 255.0 * (1.0 - coverage);
                let alpha = coverage + background;
                for channel in 0..3 {
                    pixel[channel] = ((color[channel] as f32 * coverage
                        + pixel[channel] as f32 * background)
                        / alpha)
                        .round() as u8;
                }
                pixel[3] = (alpha * 255.0).round() as u8;
            }
        }
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_indicator_is_visible_without_changing_the_application_icon() {
        let plain = icon();
        let clear = icon_with_unread(false);
        let unread = icon_with_unread(true);
        assert_eq!(plain.rgba, clear.rgba);
        assert_eq!((unread.width, unread.height), (plain.width, plain.height));
        assert_eq!(
            unread.rgba.len(),
            (unread.width * unread.height * 4) as usize
        );
        fn pixel(data: &egui::IconData, x: u32, y: u32) -> &[u8] {
            let offset = ((y * data.width + x) * 4) as usize;
            &data.rgba[offset..offset + 4]
        }
        assert_eq!(pixel(&unread, 50, 14), [240, 76, 91, 255]);
        assert_eq!(pixel(&unread, 50, 3), [242, 245, 242, 255]);
        assert_eq!(pixel(&unread, 0, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&unread, 21, 30), pixel(&plain, 21, 30));
        assert_eq!(icon().rgba, plain.rgba);
        assert_eq!(icon_with_unread(false).rgba, plain.rgba);
    }
}
