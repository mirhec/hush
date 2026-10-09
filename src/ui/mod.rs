#[cfg(test)]
mod capture;
mod demo;
mod icons;
#[cfg(test)]
mod layout_tests;
mod settings;
mod theme;
mod toast;

use crate::{
    engine,
    filter::safe_web_url,
    model::{Config, Event, Kind, Repo, RuntimeStatus},
    storage::{Paths, Store},
    tray,
};
use chrono::{Local, Utc};
use eframe::egui::{
    self, Align, Align2, FontId, Layout, Rect, RichText, Sense, Stroke, StrokeKind, Ui, UiBuilder,
    pos2, vec2,
};
use icons::Icon;
use std::{
    sync::mpsc::{self, Receiver},
    time::{Duration, Instant},
};
use theme::{Palette, label, primary, section};
use toast::Toast;
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Inbox,
    Archive,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Notifications,
    Account,
    Diagnostics,
}

pub struct HushApp {
    paths: Option<Paths>,
    store: Option<Store>,
    demo: bool,
    config: Config,
    draft: Config,
    events: Vec<Event>,
    status: RuntimeStatus,
    page: Page,
    settings_tab: SettingsTab,
    kind: Option<Kind>,
    unread_only: bool,
    search: String,
    selected: Option<String>,
    repos_text: String,
    teams_text: String,
    token: String,
    details_token: String,
    light: bool,
    message: Option<Toast>,
    job: Option<Receiver<Result<Config, String>>>,
    last_poll: Instant,
    confirm_disconnect: bool,
    confirm_clear: bool,
    focus_search: bool,
    tray: Option<tray::Tray>,
    tray_available: bool,
    tray_error: Option<String>,
    start_hidden: bool,
    service_job: Option<Receiver<Result<(), String>>>,
    running: bool,
    quitting: bool,
    allow_exit: bool,
}
impl HushApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        paths: Option<Paths>,
        store: Option<Store>,
        config: Config,
        demo: bool,
        start_hidden: bool,
    ) -> Self {
        theme::apply(&cc.egui_ctx, false);
        let cfg = if demo { demo::config() } else { config };
        let events = if demo {
            demo::events()
        } else {
            store
                .as_ref()
                .and_then(|s| s.events().ok())
                .unwrap_or_default()
        };
        let status = RuntimeStatus::default();
        let page = if cfg.login.is_empty() {
            Page::Settings
        } else {
            Page::Inbox
        };
        let app = Self {
            paths,
            store,
            demo,
            config: cfg.clone(),
            draft: cfg.clone(),
            events,
            status,
            page,
            settings_tab: if cfg.login.is_empty() {
                SettingsTab::Account
            } else {
                SettingsTab::Notifications
            },
            kind: None,
            unread_only: false,
            search: String::new(),
            selected: None,
            repos_text: repo_text(&cfg.repositories),
            teams_text: repo_text(&cfg.manual_teams),
            token: String::new(),
            details_token: String::new(),
            light: false,
            message: None,
            job: None,
            last_poll: Instant::now() - Duration::from_secs(2),
            confirm_disconnect: false,
            confirm_clear: false,
            focus_search: false,
            tray: None,
            tray_available: false,
            tray_error: None,
            start_hidden,
            service_job: None,
            running: false,
            quitting: false,
            allow_exit: false,
        };
        #[cfg(not(target_os = "linux"))]
        let mut app = app;
        #[cfg(not(target_os = "linux"))]
        if !demo {
            if let Some(paths) = &app.paths {
                match tray::Tray::new(&cc.egui_ctx, paths.root.join("tray")) {
                    Ok(tray) => app.tray = Some(tray),
                    Err(error) => {
                        app.tray_error = Some(format!("{error:#}"));
                        tray::show_window(&cc.egui_ctx);
                    }
                }
            }
            app.start_service(&cc.egui_ctx);
        }
        app
    }
    fn palette(&self) -> Palette {
        if self.light {
            Palette::light()
        } else {
            Palette::dark()
        }
    }
    fn reset_draft(&mut self) {
        self.draft = self.config.clone();
        self.repos_text = repo_text(&self.config.repositories);
        self.teams_text = repo_text(&self.config.manual_teams);
    }
    fn report(&mut self, r: anyhow::Result<()>, ok: &str) {
        self.message = Some(match r {
            Ok(()) => Toast::new(ok.into(), false),
            Err(e) => Toast::new(format!("{e:#}"), true),
        });
    }
    fn start_service(&mut self, ctx: &egui::Context) {
        if self.service_job.is_some() || self.demo || self.quitting {
            return;
        }
        let Some(paths) = self.paths.clone() else {
            return;
        };
        let (tx, rx) = mpsc::channel();
        self.service_job = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(engine::launch(&paths).map_err(|error| format!("{error:#}")));
            ctx.request_repaint();
        });
    }
    fn quit(&mut self, ctx: &egui::Context) {
        let r = self.store.as_ref().map_or(Ok(()), |s| {
            s.request_stop()?;
            #[cfg(target_os = "linux")]
            s.put("quit_desktop", "1")?;
            Ok(())
        });
        if r.is_ok() {
            self.quitting = true;
        }
        self.report(r, "Hush wird beendet …");
        tray::show_window(ctx);
    }
    fn poll(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.service_job {
            match rx.try_recv() {
                Ok(result) => {
                    self.service_job = None;
                    if let Err(error) = result {
                        self.start_hidden = false;
                        self.message = Some(Toast::new(error, true));
                        tray::show_window(ctx);
                    }
                    if self.quitting {
                        let r = self.store.as_ref().map_or(Ok(()), |s| s.request_stop());
                        if let Err(error) = r {
                            self.quitting = false;
                            self.message = Some(Toast::new(format!("{error:#}"), true));
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.service_job = None;
                    self.start_hidden = false;
                    self.message = Some(Toast::new("Dienststart wurde abgebrochen.".into(), true));
                    tray::show_window(ctx);
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.job {
            match rx.try_recv() {
                Ok(result) => {
                    self.job = None;
                    match result {
                        Ok(cfg) => {
                            self.config = cfg;
                            self.reset_draft();
                            self.page = if self.config.login.is_empty() {
                                Page::Settings
                            } else {
                                Page::Inbox
                            };
                            self.message = Some(Toast::new(
                                if self.config.login.is_empty() {
                                    "Verbindung getrennt. Lokale Ereignisse wurden entfernt."
                                } else {
                                    "Mit GitHub verbunden."
                                }
                                .into(),
                                false,
                            ));
                            self.start_service(ctx);
                        }
                        Err(e) => self.message = Some(Toast::new(e, true)),
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.job = None;
                    self.message = Some(Toast::new(
                        "Die Kontoaktion wurde abgebrochen. Bitte erneut versuchen.".into(),
                        true,
                    ));
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if !self.demo && self.last_poll.elapsed() > Duration::from_secs(1) {
            self.last_poll = Instant::now();
            if let Some(paths) = &self.paths {
                match engine::is_running(paths) {
                    Ok(running) => self.running = running,
                    Err(e) => self.message = Some(Toast::new(format!("{e:#}"), true)),
                }
            }
            if let Some(store) = &mut self.store {
                #[cfg(target_os = "linux")]
                {
                    match crate::linux_desktop::status(store) {
                        Ok(status) => {
                            self.tray_available = status.tray_available;
                            self.tray_error = status.error;
                        }
                        Err(error) => self.message = Some(Toast::new(format!("{error:#}"), true)),
                    }
                    match store.take_flag("quit_window") {
                        Ok(true) => self.quitting = true,
                        Ok(false) => {}
                        Err(error) => self.message = Some(Toast::new(format!("{error:#}"), true)),
                    }
                    match store.take_flag("show_settings") {
                        Ok(true) => {
                            self.page = Page::Settings;
                            self.selected = None;
                            tray::show_window(ctx);
                        }
                        Ok(false) => {}
                        Err(error) => self.message = Some(Toast::new(format!("{error:#}"), true)),
                    }
                    if let Ok(config) = store.config() {
                        self.draft.paused_until = config.paused_until;
                        self.config = config;
                    }
                }
                match store.take_flag("show_window") {
                    Ok(true) => {
                        self.start_hidden = false;
                        tray::show_window(ctx);
                    }
                    Ok(false) => {}
                    Err(error) => self.message = Some(Toast::new(format!("{error:#}"), true)),
                }
                match store.events() {
                    Ok(events) => self.events = events,
                    Err(e) => self.message = Some(Toast::new(e.to_string(), true)),
                }
                match store.status() {
                    Ok(status) => self.status = status,
                    Err(e) => self.message = Some(Toast::new(e.to_string(), true)),
                }
            }
        }
    }
    fn select(&mut self, id: String) {
        if let Some(e) = self.events.iter_mut().find(|e| e.id == id) {
            e.unread = false;
        }
        if let Some(store) = &self.store {
            if let Err(e) = store.read(&id) {
                self.message = Some(Toast::new(e.to_string(), true));
            }
        }
        self.selected = Some(id);
    }
    fn archive(&mut self, id: &str, value: bool) {
        let result = if let Some(s) = &self.store {
            s.archive(id, value)
        } else {
            Ok(())
        };
        if result.is_ok() {
            if let Some(e) = self.events.iter_mut().find(|e| e.id == id) {
                e.archived = value;
                e.unread = false;
            }
            self.selected = None;
        }
        self.report(
            result,
            if value {
                "Als erledigt markiert."
            } else {
                "Zurück im Posteingang."
            },
        );
    }
    fn save_settings(&mut self) {
        let result = (|| -> anyhow::Result<Config> {
            let mut c = self.draft.clone();
            c.repositories = Repo::list(&self.repos_text)?;
            c.manual_teams = Repo::list(&self.teams_text)?;
            c.validate()?;
            // Account and pause are managed separately, not overwritten by an old settings draft.
            c.login = self.config.login.clone();
            c.has_detail_token = self.config.has_detail_token;
            c.read_org = self.config.read_org;
            c.paused_until = self.config.paused_until;
            if let Some(s) = &mut self.store {
                s.save_config(&c)
            } else {
                Ok(c)
            }
        })();
        match result {
            Ok(c) => {
                self.config = c;
                self.reset_draft();
                self.message = Some(Toast::new(
                    if self.demo {
                        "Einstellungen nur in dieser Demo geändert."
                    } else {
                        "Einstellungen gespeichert."
                    }
                    .into(),
                    false,
                ));
            }
            Err(e) => self.message = Some(Toast::new(e.to_string(), true)),
        }
    }
    fn account_job(&mut self, disconnect: bool) {
        if self.demo || self.job.is_some() {
            return;
        }
        let Some(paths) = self.paths.clone() else {
            return;
        };
        let token = Zeroizing::new(std::mem::take(&mut self.token));
        let details = if self.details_token.trim().is_empty() {
            self.details_token.zeroize();
            None
        } else {
            Some(Zeroizing::new(std::mem::take(&mut self.details_token)))
        };
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.message = None;
        std::thread::spawn(move || {
            let r = if disconnect {
                engine::disconnect(&paths).map(|_| Config::default())
            } else {
                engine::connect(&paths, token, details)
            };
            let _ = tx.send(r.map_err(|e| e.to_string()));
        });
    }
    fn pause(&mut self) {
        let mut c = self.config.clone();
        c.paused_until = if c.paused() {
            0
        } else {
            Utc::now().timestamp() + 1800
        };
        let result = if let Some(s) = &mut self.store {
            s.save_config(&c)
        } else {
            Ok(c)
        };
        match result {
            Ok(c) => {
                self.config = c;
                self.draft.paused_until = self.config.paused_until;
            }
            Err(e) => self.message = Some(Toast::new(e.to_string(), true)),
        }
    }
    fn open(&mut self, url: &str) {
        let r = (|| -> anyhow::Result<()> {
            let u = safe_web_url(url)?;
            webbrowser::open(u.as_str())?;
            Ok(())
        })();
        if let Err(e) = r {
            self.message = Some(Toast::new(e.to_string(), true));
        }
    }
    fn sidebar(&mut self, ui: &mut Ui) {
        let p = self.palette();
        let compact = ui.available_width() < 100.;
        let (brand, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.), Sense::hover());
        let mark = Rect::from_min_size(brand.min + vec2(4., 4.), vec2(28., 28.));
        ui.painter().rect_filled(mark, 7, p.accent);
        icons::mark(ui.painter(), mark.center(), 18., p.on_accent);
        if !compact {
            ui.painter().text(
                brand.min + vec2(42., 17.),
                Align2::LEFT_CENTER,
                "hush",
                FontId::proportional(22.),
                p.text,
            );
        }
        ui.add_space(8.);
        let unread = self
            .events
            .iter()
            .filter(|e| e.unread && !e.archived)
            .count();
        if nav(
            ui,
            Icon::Inbox,
            "Posteingang",
            Some(unread),
            self.page == Page::Inbox && self.kind.is_none(),
            p,
        ) {
            self.page = Page::Inbox;
            self.kind = None;
            self.selected = None;
        }
        if nav(
            ui,
            Icon::Archive,
            "Erledigt",
            None,
            self.page == Page::Archive,
            p,
        ) {
            self.page = Page::Archive;
            self.kind = None;
            self.selected = None;
        }
        ui.add_space(8.);
        ui.separator();
        if !compact {
            label(ui, "Filter", 11., p.faint);
        }
        for kind in Kind::ALL {
            let n = self
                .events
                .iter()
                .filter(|e| e.kind == kind && e.unread && !e.archived)
                .count();
            if nav(
                ui,
                kind.into(),
                kind.label(),
                Some(n),
                self.page == Page::Inbox && self.kind == Some(kind),
                p,
            ) {
                self.page = Page::Inbox;
                self.kind = Some(kind);
                self.selected = None;
            }
        }
        ui.add_space((ui.available_height() - 82.).max(8.));
        if nav(
            ui,
            Icon::Settings,
            "Einstellungen",
            None,
            self.page == Page::Settings,
            p,
        ) {
            if self.page != Page::Settings {
                self.reset_draft();
            }
            self.page = Page::Settings;
            self.selected = None;
        }
        ui.separator();
        let account = if self.config.login.is_empty() {
            "Nicht verbunden".into()
        } else {
            format!("@{}", self.config.login)
        };
        let (r, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "GitHub-Konto verwalten")
        });
        let avatar = Rect::from_min_size(r.min + vec2(6., 2.), vec2(22., 22.));
        ui.painter().rect_filled(avatar, 6, p.hover);
        let initial = self
            .config
            .login
            .chars()
            .next()
            .unwrap_or('H')
            .to_uppercase()
            .to_string();
        ui.painter().text(
            avatar.center(),
            Align2::CENTER_CENTER,
            initial,
            FontId::proportional(11.),
            p.accent,
        );
        if !compact {
            paint_elided(
                ui,
                pos2(r.left() + 36., r.top() + 5.),
                &account,
                12.,
                p.muted,
                r.width() - 40.,
            );
        }
        if response.on_hover_text("GitHub-Konto verwalten").clicked() {
            if self.page != Page::Settings {
                self.reset_draft();
            }
            self.page = Page::Settings;
            self.settings_tab = SettingsTab::Account;
            self.selected = None;
        }
    }

    fn header(&mut self, ui: &mut Ui) {
        let p = self.palette();
        ui.horizontal(|ui| {
            let title = match self.page {
                Page::Settings => "Einstellungen",
                Page::Archive => "Erledigt",
                Page::Inbox => self.kind.map_or("Posteingang", Kind::label),
            };
            label(ui, title, 20., p.text);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(
                    ui,
                    if self.light { Icon::Moon } else { Icon::Sun },
                    "Helles / dunkles Erscheinungsbild (dieses Fenster)",
                    p,
                )
                .clicked()
                {
                    self.light = !self.light;
                    theme::apply(ui.ctx(), self.light);
                }
                if icons::button(
                    ui,
                    if self.config.paused() {
                        Icon::Play
                    } else {
                        Icon::Pause
                    },
                    if self.config.paused() {
                        "Benachrichtigungen fortsetzen"
                    } else {
                        "Benachrichtigungen 30 Minuten pausieren"
                    },
                    p,
                )
                .clicked()
                {
                    self.pause();
                }
                if icons::button(ui, Icon::Refresh, "Jetzt aktualisieren", p).clicked() {
                    let r = if let Some(s) = &self.store {
                        s.request_refresh()
                    } else {
                        Ok(())
                    };
                    self.report(
                        r,
                        if self.demo {
                            "Demodaten sind bereits aktuell."
                        } else {
                            "Aktualisierung angefragt."
                        },
                    );
                }
                let status = if self.demo {
                    "Demo"
                } else if self.service_job.is_some() {
                    "Startet"
                } else if !self.running || !self.status.alive() {
                    "Inaktiv"
                } else if self.status.notification_error.is_some() {
                    "Zustellfehler"
                } else if self.config.paused() {
                    "Pausiert"
                } else if self.config.login.is_empty() {
                    "Offline"
                } else if self.status.warnings.is_empty() && self.status.last_sync.is_some() {
                    "Verbunden"
                } else {
                    "Prüfen"
                };
                let color = if self.status.notification_error.is_some() {
                    p.danger
                } else if self.demo
                    || self.config.paused()
                    || !self.status.warnings.is_empty()
                    || !self.running
                    || !self.status.alive()
                    || self.config.login.is_empty()
                {
                    p.amber
                } else {
                    p.accent
                };
                let (dot, response) = ui.allocate_exact_size(vec2(10., 16.), Sense::hover());
                ui.painter().circle_filled(dot.center(), 3., color);
                response.on_hover_text(status);
                if ui.available_width() > 90. {
                    label(ui, status, 11., color);
                }
            });
        });
        ui.add_space(4.);
        ui.separator();
        ui.add_space(2.);
    }
    fn notice(&mut self, ui: &mut Ui) {
        let p = self.palette();
        if !self.demo && (!self.running || !self.status.alive()) {
            ui.horizontal_wrapped(|ui| {
                label(
                    ui,
                    if self.service_job.is_some() {
                        "Hintergrunddienst startet …"
                    } else {
                        "Hintergrunddienst ist inaktiv."
                    },
                    12.,
                    p.amber,
                );
                if ui
                    .add_enabled(
                        self.service_job.is_none() && !self.quitting,
                        egui::Button::new("Dienst starten"),
                    )
                    .clicked()
                {
                    self.start_service(ui.ctx());
                }
            });
            if let Some(error) = &self.status.service_error {
                label(ui, error, 12., p.danger);
            }
            ui.add_space(10.);
        }
        let access_error = self
            .status
            .repositories
            .iter()
            .any(|repo| repo.error.is_some())
            || self
                .status
                .warnings
                .iter()
                .any(|warning| warning.contains("HTTP 403") || warning.contains("HTTP 404"));
        if !self.demo && access_error {
            ui.horizontal_wrapped(|ui| {
                label(ui, if self.config.has_detail_token {
                    "GitHub-Zugriff fehlgeschlagen. Repository-Auswahl und Token-Rechte prüfen."
                } else {
                    "Repository-Zugriff fehlgeschlagen. Für private Repositories einen Detail-Token hinterlegen."
                }, 12., p.amber);
                if (self.page != Page::Settings || self.settings_tab != SettingsTab::Account)
                    && ui.small_button("Zugriff prüfen").clicked() {
                    if self.page != Page::Settings { self.reset_draft(); }
                    self.page = Page::Settings;
                    self.settings_tab = SettingsTab::Account;
                }
            });
            ui.add_space(8.);
        }
        if let Some(error) = &self.status.notification_error {
            label(ui, error, 12., p.danger);
            ui.add_space(8.);
        }
        if let Some(error) = &self.tray_error {
            label(ui, error, 11.5, p.amber);
            ui.add_space(8.);
        }
    }
    fn inbox(&mut self, ui: &mut Ui) {
        let p = self.palette();
        let archived = self.page == Page::Archive;
        let narrow = ui.available_width() < 470.;
        ui.horizontal(|ui| {
            let search_width = if narrow {
                ui.available_width()
            } else {
                (ui.available_width() - 210.).max(100.)
            };
            let response = ui.add_sized(
                [search_width, 28.],
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Suchen …")
                    .margin(vec2(8., 5.)),
            );
            if self.focus_search {
                response.request_focus();
                self.focus_search = false;
            }
            if !narrow {
                self.inbox_actions(ui);
            }
        });
        if narrow {
            ui.horizontal(|ui| self.inbox_actions(ui));
        }
        ui.add_space(4.);
        let needle = self.search.to_lowercase();
        let visible: Vec<Event> = self
            .events
            .iter()
            .filter(|e| {
                e.archived == archived
                    && self.kind.is_none_or(|k| e.kind == k)
                    && (!self.unread_only || e.unread)
                    && format!("{} {} {} {}", e.title, e.repository, e.actor, e.detail)
                        .to_lowercase()
                        .contains(&needle)
            })
            .cloned()
            .collect();
        if visible.is_empty() {
            ui.add_space(36.);
            ui.vertical_centered(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(40., 40.), Sense::hover());
                ui.painter()
                    .circle_stroke(r.center(), 19., Stroke::new(1., p.border));
                icons::mark(ui.painter(), r.center(), 22., p.accent);
                ui.add_space(10.);
                label(
                    ui,
                    if self.search.is_empty() {
                        "Keine Benachrichtigungen"
                    } else {
                        "Keine Treffer"
                    },
                    18.,
                    p.text,
                );
                label(
                    ui,
                    if self.search.is_empty() {
                        "Neue Benachrichtigungen erscheinen hier."
                    } else {
                        "Versuche einen anderen Namen oder ein Repository."
                    },
                    13.,
                    p.muted,
                );
            });
            return;
        }
        let list_height = ui.available_height();
        egui::ScrollArea::vertical()
            .id_salt("inbox-scroll")
            .max_height(list_height.max(40.))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let mut last_date = String::new();
                for e in visible {
                    let date = e
                        .occurred_at
                        .with_timezone(&Local)
                        .format("%d.%m.%Y")
                        .to_string();
                    if date != last_date {
                        if !last_date.is_empty() {
                            ui.add_space(8.);
                        }
                        ui.add_space(5.);
                        label(
                            ui,
                            if e.occurred_at.with_timezone(&Local).date_naive()
                                == Local::now().date_naive()
                            {
                                "HEUTE".into()
                            } else {
                                date.clone()
                            },
                            10.,
                            p.faint,
                        );
                        ui.add_space(5.);
                        last_date = date;
                    }
                    let selected = self.selected.as_deref() == Some(e.id.as_str());
                    if event_card(ui, &e, selected, p).clicked() {
                        self.select(e.id.clone());
                    }
                }
            });
    }
    fn inbox_actions(&mut self, ui: &mut Ui) {
        ui.checkbox(&mut self.unread_only, "Ungelesen");
        if ui.button("Alles gelesen").clicked() {
            let result = self.store.as_ref().map_or(Ok(()), |store| store.read_all());
            if result.is_ok() {
                for event in &mut self.events {
                    event.unread = false;
                }
            }
            self.report(result, "Alle als gelesen markiert.");
        }
    }
    fn detail(&mut self, ui: &mut Ui, event: &Event) {
        let p = self.palette();
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(16., 16.), Sense::hover());
            icons::paint(ui.painter(), r, event.kind.into(), p.kind(event.kind));
            label(ui, event.kind.short(), 12., p.kind(event.kind));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(ui, Icon::Close, "Detailansicht schließen", p).clicked() {
                    self.selected = None;
                }
            });
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt("event-details")
            .max_height((ui.available_height() - 48.).max(40.))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.);
                label(ui, &event.title, 20., p.text);
                ui.add_space(2.);
                label(ui, &event.repository, 12., p.muted);
                label(
                    ui,
                    format!(
                        "@{} · {}",
                        event.actor,
                        event
                            .occurred_at
                            .with_timezone(&Local)
                            .format("%d.%m. %H:%M")
                    ),
                    11.,
                    p.faint,
                );
                ui.add_space(10.);
                ui.separator();
                ui.add_space(6.);
                label(
                    ui,
                    if event.detail.is_empty() {
                        "Kein zusätzlicher Kommentar."
                    } else {
                        &event.detail
                    },
                    13.,
                    p.text,
                );
            });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if primary(ui, "Auf GitHub ↗", p).clicked() {
                self.open(&event.url);
            }
            if ui
                .button(if event.archived {
                    "Wiederherstellen"
                } else {
                    "Erledigen"
                })
                .on_hover_text("Ändert nur den lokalen Posteingang.")
                .clicked()
            {
                self.archive(&event.id, !event.archived);
            }
        });
    }
}
impl eframe::App for HushApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        ctx.request_repaint_after(Duration::from_millis(500));
        let events: Vec<_> = self
            .tray
            .as_ref()
            .map(|tray| tray.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                tray::Event::Available(available) => {
                    self.tray_available = available;
                    self.tray_error = if available {
                        None
                    } else {
                        Some(
                            "Keine Tray-Leiste verfügbar. In Niri/DMS das System-Tray aktivieren."
                                .into(),
                        )
                    };
                    if !available {
                        self.start_hidden = false;
                        tray::show_window(ctx);
                    } else if self.start_hidden {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                        self.start_hidden = false;
                    }
                }
                tray::Event::Error(error) => {
                    self.tray_available = false;
                    self.start_hidden = false;
                    self.tray_error = Some(error);
                    tray::show_window(ctx);
                }
                tray::Event::Open => {
                    self.start_hidden = false;
                    tray::show_window(ctx);
                }
                tray::Event::Settings => {
                    self.start_hidden = false;
                    self.page = Page::Settings;
                    self.selected = None;
                    tray::show_window(ctx);
                }
                tray::Event::Refresh => {
                    let r = self.store.as_ref().map_or(Ok(()), |s| s.request_refresh());
                    self.report(r, "Aktualisierung angefragt.");
                }
                tray::Event::Pause => self.pause(),
                tray::Event::Start => self.start_service(ctx),
                tray::Event::Quit => self.quit(ctx),
            }
        }
        if self.quitting
            && self.service_job.is_none()
            && self.job.is_none()
            && self
                .paths
                .as_ref()
                .is_none_or(|paths| matches!(engine::is_running(paths), Ok(false)))
        {
            self.allow_exit = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Wayland has no set_visible support. On Linux, closing destroys this
        // window process; the independent tray process can open a fresh window.
        #[cfg(not(target_os = "linux"))]
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_exit
            && (self.tray_available || self.quitting)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        if let Some(tray) = &mut self.tray {
            tray.update(tray::State {
                running: self.running && self.status.alive(),
                paused: self.config.paused(),
                unread: self
                    .events
                    .iter()
                    .filter(|e| e.unread && !e.archived)
                    .count(),
                warning: self.status.notification_error.is_some()
                    || !self.status.warnings.is_empty()
                    || self.status.service_error.is_some()
                    || self.config.login.is_empty(),
            });
        }
    }
    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        if root.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) {
            self.page = Page::Inbox;
            self.focus_search = true;
        }
        if root.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.selected = None;
            self.message = None;
        }
        let p = self.palette();
        let rect = root.max_rect();
        root.painter().rect_filled(rect, 0, p.bg);
        let side_width = if rect.width() < 840. { 56. } else { 188. };
        let side = Rect::from_min_max(rect.min, pos2(rect.left() + side_width, rect.bottom()));
        root.painter().rect_filled(side, 0, p.sidebar);
        root.painter().line_segment(
            [side.right_top(), side.right_bottom()],
            Stroke::new(1., p.border),
        );
        let mut side_ui = root.new_child(
            UiBuilder::new()
                .id_salt("sidebar")
                .max_rect(side.shrink2(vec2(8., 12.)))
                .layout(Layout::top_down(Align::Min)),
        );
        side_ui.set_clip_rect(side);
        self.sidebar(&mut side_ui);
        let main = Rect::from_min_max(
            pos2(side.right() + 16., rect.top() + 12.),
            rect.right_bottom() - vec2(16., 12.),
        );
        let selected = self
            .selected
            .as_ref()
            .and_then(|id| self.events.iter().find(|e| &e.id == id))
            .cloned();
        let split = self.page != Page::Settings && selected.is_some() && rect.width() >= 980.;
        let content = if split {
            Rect::from_min_max(main.min, pos2(main.right() - 344., main.bottom()))
        } else {
            main
        };
        let mut ui = root.new_child(
            UiBuilder::new()
                .id_salt("main")
                .max_rect(content)
                .layout(Layout::top_down(Align::Min)),
        );
        ui.set_clip_rect(content);
        self.header(&mut ui);
        self.notice(&mut ui);
        if self.page == Page::Settings {
            self.settings(&mut ui);
        } else if selected.is_some() && !split {
            self.detail(&mut ui, &selected.clone().unwrap());
        } else {
            self.inbox(&mut ui);
        }
        if split {
            let detail = Rect::from_min_max(pos2(main.right() - 312., main.top()), main.max);
            root.painter().line_segment(
                [
                    pos2(detail.left() - 16., rect.top()),
                    pos2(detail.left() - 16., rect.bottom()),
                ],
                Stroke::new(1., p.border),
            );
            let mut detail_ui = root.new_child(
                UiBuilder::new()
                    .id_salt("details")
                    .max_rect(detail)
                    .layout(Layout::top_down(Align::Min)),
            );
            detail_ui.set_clip_rect(detail);
            self.detail(&mut detail_ui, &selected.unwrap());
        }
        Toast::show(&mut self.message, root.ctx(), p);
    }
}
impl Drop for HushApp {
    fn drop(&mut self) {
        self.token.zeroize();
        self.details_token.zeroize();
    }
}
fn repo_text(repos: &[Repo]) -> String {
    repos
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
fn paint_elided(
    ui: &Ui,
    position: egui::Pos2,
    text: &str,
    size: f32,
    color: egui::Color32,
    width: f32,
) {
    let mut job = egui::text::LayoutJob::simple(
        text.to_owned(),
        FontId::proportional(size),
        color,
        width.max(1.),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(position, galley, color);
}

fn nav(
    ui: &mut Ui,
    icon: Icon,
    text: &str,
    count: Option<usize>,
    selected: bool,
    p: Palette,
) -> bool {
    let (r, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.), Sense::click());
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, text));
    if selected || response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(r, 5, if selected { p.hover } else { p.card });
    }
    let color = if selected { p.accent } else { p.muted };
    let compact = r.width() < 100.;
    let center = if compact {
        r.center()
    } else {
        pos2(r.left() + 16., r.center().y)
    };
    icons::paint(
        ui.painter(),
        Rect::from_center_size(center, vec2(16., 16.)),
        icon,
        color,
    );
    if !compact {
        paint_elided(ui, r.min + vec2(32., 7.), text, 12., color, r.width() - 52.);
    }
    if let Some(n) = count.filter(|n| *n > 0) {
        if compact {
            ui.painter()
                .circle_filled(r.right_top() + vec2(-5., 5.), 2., p.accent);
        } else {
            ui.painter().text(
                r.right_center() - vec2(8., 0.),
                Align2::RIGHT_CENTER,
                n.to_string(),
                FontId::proportional(10.),
                p.faint,
            );
        }
    }
    response
        .on_hover_text(if let Some(n) = count {
            format!("{text} · {n} ungelesen")
        } else {
            text.into()
        })
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}

fn setting_toggle(
    ui: &mut Ui,
    title: &str,
    help: &str,
    value: &mut bool,
    p: Palette,
    icon: Option<Icon>,
) -> egui::InnerResponse<egui::Response> {
    let width = ui.available_width();
    egui::Frame::new()
        .fill(p.card)
        .inner_margin(egui::Margin::symmetric(10, 6))
        .corner_radius(4)
        .show(ui, |ui| {
            ui.set_width((width - 20.).max(0.));
            ui.horizontal(|ui| {
                if let Some(icon) = icon {
                    let (r, _) = ui.allocate_exact_size(vec2(16., 16.), Sense::hover());
                    icons::paint(ui.painter(), r, icon, p.muted);
                }
                let text_width = (ui.available_width() - 32. - ui.spacing().item_spacing.x).max(1.);
                ui.allocate_ui_with_layout(
                    vec2(text_width, 24.),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_width(text_width);
                        ui.add(
                            egui::Label::new(RichText::new(title).size(13.).color(p.text))
                                .truncate(),
                        )
                        .on_hover_text(format!("{title}\n{help}"));
                    },
                );
                let (hit, mut response) = ui.allocate_exact_size(vec2(32., 24.), Sense::click());
                if response.clicked() {
                    response.request_focus();
                    *value = !*value;
                    response.mark_changed();
                }
                response.widget_info(|| {
                    egui::WidgetInfo::selected(egui::WidgetType::Checkbox, true, *value, title)
                });
                let r = Rect::from_center_size(hit.center(), vec2(32., 18.));
                let t = ui.ctx().animate_bool(response.id, *value);
                ui.painter()
                    .rect_filled(r, 9, if *value { p.accent } else { p.border });
                let x = egui::lerp((r.left() + 9.)..=(r.right() - 9.), t);
                ui.painter().circle_filled(
                    pos2(x, r.center().y),
                    6.,
                    if *value { p.on_accent } else { p.muted },
                );
                if response.has_focus() {
                    ui.painter().rect_stroke(
                        r.expand(2.),
                        10,
                        Stroke::new(1., p.accent),
                        StrokeKind::Outside,
                    );
                }
                response
                    .on_hover_text(help)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
            })
            .inner
        })
}

fn event_card(ui: &mut Ui, event: &Event, selected: bool, p: Palette) -> egui::Response {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 62.), Sense::hover());
    let response = ui.interact(r, ui.id().with(&event.id), Sense::click());
    if selected || response.hovered() || response.has_focus() {
        ui.painter()
            .rect_filled(r, 4, if selected { p.hover } else { p.card });
    }
    ui.painter().line_segment(
        [r.left_bottom(), r.right_bottom()],
        Stroke::new(1., p.border),
    );
    if selected {
        ui.painter().rect_filled(
            Rect::from_min_size(r.min + vec2(0., 12.), vec2(2., 38.)),
            1,
            p.accent,
        );
    }
    icons::paint(
        ui.painter(),
        Rect::from_min_size(r.min + vec2(12., 12.), vec2(16., 16.)),
        event.kind.into(),
        p.kind(event.kind),
    );
    paint_elided(
        ui,
        r.min + vec2(38., 10.),
        &event.title,
        14.,
        if event.unread { p.text } else { p.muted },
        r.width() - 120.,
    );
    paint_elided(
        ui,
        r.min + vec2(38., 35.),
        &format!("{}  ·  @{}", event.repository, event.actor),
        11.,
        p.faint,
        r.width() - 50.,
    );
    ui.painter().text(
        r.right_top() + vec2(-10., 14.),
        Align2::RIGHT_TOP,
        age(event),
        FontId::proportional(10.5),
        p.faint,
    );
    if event.unread {
        ui.painter()
            .circle_filled(pos2(r.left() + 20., r.top() + 43.), 2.5, p.accent);
    }
    if response.clicked() {
        response.request_focus();
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("{}: {}", event.kind.short(), event.title),
        )
    });
    response
        .on_hover_text(format!(
            "{}\n{}\n{} · @{}",
            event.kind.short(),
            event.title,
            event.repository,
            event.actor
        ))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn age(event: &Event) -> String {
    let m = (Utc::now() - event.occurred_at).num_minutes().max(0);
    if m < 1 {
        "Gerade eben".into()
    } else if m < 60 {
        format!("{m} Min.")
    } else if m < 1440 {
        format!("{} Std.", m / 60)
    } else {
        event
            .occurred_at
            .with_timezone(&Local)
            .format("%d.%m.")
            .to_string()
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_window_tests {
    use super::*;
    use eframe::App;

    fn window(paths: &Paths, ctx: &egui::Context) -> HushApp {
        let store = Store::open(paths).unwrap();
        let config = store.config().unwrap();
        HushApp::new(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            Some(paths.clone()),
            Some(store),
            config,
            false,
            false,
        )
    }

    #[test]
    fn linux_close_with_tray_destroys_window_without_stopping_service() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths::at(temp.path().join("data")).unwrap();
        let mut store = Store::open(&paths).unwrap();
        store
            .put("desktop_status", r#"{"tray_available":true,"error":null}"#)
            .unwrap();
        let ctx = egui::Context::default();
        let mut app = window(&paths, &ctx);
        assert!(app.tray.is_none(), "Linux window must not own the tray");
        assert!(
            app.service_job.is_none(),
            "Reopening must not restart a stopped service"
        );
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let output = ctx.run_logic(&input, |ctx| {
            app.logic(ctx, &mut eframe::Frame::_new_kittest())
        });
        assert!(app.tray_available);
        for command in output.viewport_commands.values().flatten() {
            assert!(
                !matches!(
                    command,
                    egui::ViewportCommand::CancelClose | egui::ViewportCommand::Visible(false)
                ),
                "Linux close must not be cancelled/hidden: {command:?}"
            );
        }
        assert!(!store.take_flag("shutdown").unwrap());
        assert!(!store.take_flag("quit_desktop").unwrap());
        // A new real app instance restores settings after the old window exits.
        drop(app);
        let mut config = store.config().unwrap();
        config.paused_until = Utc::now().timestamp() + 1800;
        store.save_config(&config).unwrap();
        let reopened = window(&paths, &ctx);
        assert!(reopened.config.paused());
        assert!(reopened.service_job.is_none());
    }

    #[test]
    fn controller_actions_reach_open_window_and_explicit_quit_closes_it() {
        let temp = tempfile::tempdir().unwrap();
        let paths = Paths::at(temp.path().join("data")).unwrap();
        let mut store = Store::open(&paths).unwrap();
        let ctx = egui::Context::default();
        let mut app = window(&paths, &ctx);
        let mut config = store.config().unwrap();
        config.paused_until = Utc::now().timestamp() + 1800;
        store.save_config(&config).unwrap();
        store.put("show_settings", "1").unwrap();
        store.put("quit_window", "1").unwrap();
        let output = ctx.run_logic(&egui::RawInput::default(), |ctx| {
            app.logic(ctx, &mut eframe::Frame::_new_kittest())
        });
        assert!(app.config.paused());
        assert!(app.page == Page::Settings);
        assert!(
            output
                .viewport_commands
                .values()
                .flatten()
                .any(|command| matches!(command, egui::ViewportCommand::Close))
        );
        assert!(
            !output
                .viewport_commands
                .values()
                .flatten()
                .any(|command| matches!(command, egui::ViewportCommand::CancelClose))
        );
        app.quit(&ctx);
        assert!(store.take_flag("quit_desktop").unwrap());
        assert!(store.take_flag("shutdown").unwrap());
    }
}
