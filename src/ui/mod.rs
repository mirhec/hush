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
    oauth,
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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
use theme::{Palette, label, primary, section};
use toast::Toast;
use zeroize::{Zeroize, Zeroizing};

const INBOX_LIMIT: usize = 20;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Inbox,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsTab {
    Notifications,
    Account,
    Diagnostics,
}

enum OAuthMessage {
    Code {
        user_code: String,
        verification_uri: String,
        expires_at: Instant,
    },
    Finished(Result<oauth::Credentials, String>),
}
struct OAuthLogin {
    events: Receiver<OAuthMessage>,
    cancel: Arc<AtomicBool>,
    user_code: Option<String>,
    verification_uri: Option<String>,
    expires_at: Option<Instant>,
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
    open_url: fn(&str) -> std::io::Result<()>,
    repos_text: String,
    teams_text: String,
    token: String,
    details_token: String,
    light: bool,
    message: Option<Toast>,
    job: Option<Receiver<Result<Config, String>>>,
    oauth_login: Option<OAuthLogin>,
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
            open_url: webbrowser::open,
            repos_text: repo_text(&cfg.repositories),
            teams_text: repo_text(&cfg.manual_teams),
            token: String::new(),
            details_token: String::new(),
            light: false,
            message: None,
            job: None,
            oauth_login: None,
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
        self.cancel_oauth();
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
        self.poll_oauth();
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
                            tray::show_window(ctx);
                        }
                        Ok(false) => {}
                        Err(error) => self.message = Some(Toast::new(format!("{error:#}"), true)),
                    }
                }
                // Reload on every platform, including when a keyring cleanup failed after
                // an account change had already been committed to SQLite.
                if let Ok(config) = store.config() {
                    self.draft.paused_until = config.paused_until;
                    self.config = config;
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
    fn open_event(&mut self, event: &Event) {
        // Keep the item unread if the browser could not be opened.
        if !self.open(&event.url) {
            return;
        }
        self.read_event(event);
    }
    fn read_event(&mut self, event: &Event) {
        let result = self
            .store
            .as_ref()
            .map_or(Ok(()), |store| store.read(&event.id));
        match result {
            Ok(()) => {
                if let Some(item) = self.events.iter_mut().find(|item| item.id == event.id) {
                    item.unread = false;
                }
            }
            Err(error) => self.message = Some(Toast::new(error.to_string(), true)),
        }
    }
    fn refresh(&mut self) {
        let result = self
            .store
            .as_ref()
            .map_or(Ok(()), |store| store.request_refresh());
        self.report(
            result,
            if self.demo {
                "Demodaten sind bereits aktuell."
            } else {
                "Aktualisierung angefragt."
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
            let current = self
                .store
                .as_ref()
                .map(Store::config)
                .transpose()?
                .unwrap_or_else(|| self.config.clone());
            c.login = current.login;
            c.has_detail_token = current.has_detail_token;
            c.oauth = current.oauth;
            c.read_org = current.read_org;
            c.paused_until = current.paused_until;
            if let Some(s) = &mut self.store {
                s.save_preferences(&c)
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
    fn start_oauth(&mut self) {
        if self.demo || self.job.is_some() || self.oauth_login.is_some() {
            return;
        }
        let Some(client_id) = oauth::client_id() else {
            self.message = Some(Toast::new(
                "GitHub-Anmeldung ist in diesem Build nicht konfiguriert.".into(),
                true,
            ));
            return;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = cancel.clone();
        let (send, events) = mpsc::channel();
        self.oauth_login = Some(OAuthLogin {
            events,
            cancel,
            user_code: None,
            verification_uri: None,
            expires_at: None,
        });
        self.message = None;
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<oauth::Credentials> {
                let code = oauth::start(client_id)?;
                anyhow::ensure!(
                    !worker_cancel.load(Ordering::Relaxed),
                    "Anmeldung abgebrochen."
                );
                send.send(OAuthMessage::Code {
                    user_code: code.user_code.clone(),
                    verification_uri: code.verification_uri.clone(),
                    expires_at: Instant::now() + Duration::from_secs(code.expires_in),
                })
                .map_err(|_| anyhow::anyhow!("Anmeldung abgebrochen."))?;
                oauth::poll(client_id, &code, &worker_cancel)
            })();
            // Only the UI may start credential persistence. Closing/cancelling drops this channel
            // and its zeroizing credentials instead of completing an abandoned login in the background.
            let _ = send.send(OAuthMessage::Finished(
                result.map_err(|error| error.to_string()),
            ));
        });
    }
    fn cancel_oauth(&mut self) {
        if let Some(login) = self.oauth_login.take() {
            login.cancel.store(true, Ordering::Relaxed);
        }
    }
    fn poll_oauth(&mut self) {
        let messages: Vec<_> = self
            .oauth_login
            .as_ref()
            .map(|login| login.events.try_iter().collect())
            .unwrap_or_default();
        for message in messages {
            if self.oauth_login.is_none() {
                break;
            }
            match message {
                OAuthMessage::Code {
                    user_code,
                    verification_uri,
                    expires_at,
                } => {
                    if let Some(login) = &mut self.oauth_login {
                        login.user_code = Some(user_code);
                        login.verification_uri = Some(verification_uri.clone());
                        login.expires_at = Some(expires_at);
                    }
                    self.open(&verification_uri);
                }
                OAuthMessage::Finished(result) => {
                    self.oauth_login = None;
                    match result {
                        Ok(credentials) => {
                            let Some(paths) = self.paths.clone() else {
                                continue;
                            };
                            let (send, receive) = mpsc::channel();
                            self.job = Some(receive);
                            self.token.zeroize();
                            self.details_token.zeroize();
                            std::thread::spawn(move || {
                                let result = engine::connect_oauth(&paths, credentials);
                                let _ = send.send(result.map_err(|error| error.to_string()));
                            });
                        }
                        Err(error) => self.message = Some(Toast::new(error, true)),
                    }
                }
            }
        }
    }
    fn account_job(&mut self, disconnect: bool) {
        if self.demo || self.job.is_some() || self.oauth_login.is_some() {
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
        let result = if let Some(store) = &mut self.store {
            store.toggle_pause()
        } else {
            let mut config = self.config.clone();
            config.paused_until = if config.paused() {
                0
            } else {
                Utc::now().timestamp() + 1800
            };
            Ok(config)
        };
        match result {
            Ok(config) => {
                self.config = config;
                self.draft.paused_until = self.config.paused_until;
            }
            Err(error) => self.message = Some(Toast::new(error.to_string(), true)),
        }
    }
    fn open(&mut self, url: &str) -> bool {
        let result = (|| -> anyhow::Result<()> {
            let url = safe_web_url(url)?;
            (self.open_url)(url.as_str())?;
            Ok(())
        })();
        match result {
            Ok(()) => true,
            Err(error) => {
                self.message = Some(Toast::new(error.to_string(), true));
                false
            }
        }
    }
    fn header(&mut self, ui: &mut Ui) {
        let p = self.palette();
        ui.horizontal(|ui| {
            if self.page == Page::Settings {
                if icons::button(ui, Icon::Back, "Zurück zum Posteingang", p).clicked() {
                    self.page = Page::Inbox;
                }
                label(ui, "Einstellungen", 18., p.text);
            } else {
                label(ui, "Posteingang", 18., p.text);
                let unread = self.events.iter().filter(|event| event.unread).count();
                if unread > 0 {
                    ui.label(RichText::new(unread.to_string()).size(12.).color(p.accent))
                        .on_hover_text(format!("{unread} ungelesen"));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if self.page == Page::Inbox {
                    if icons::button(ui, Icon::Settings, "Einstellungen", p).clicked() {
                        self.page = Page::Settings;
                    }
                } else if icons::button(
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
            });
        });
        ui.add_space(2.);
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
                label(ui, if self.config.has_detail_token || self.config.oauth {
                    "GitHub-Zugriff fehlgeschlagen. Berechtigungen und Organisationsfreigaben prüfen."
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
    fn visible_events(&self) -> Vec<Event> {
        let needle = self.search.to_lowercase();
        let mut matches: Vec<_> = self
            .events
            .iter()
            .filter(|event| {
                self.kind.is_none_or(|kind| event.kind == kind)
                    && (!self.unread_only || event.unread)
                    && format!(
                        "{} {} {} {}",
                        event.title, event.repository, event.actor, event.detail
                    )
                    .to_lowercase()
                    .contains(&needle)
            })
            .collect();
        matches.sort_by_key(|event| std::cmp::Reverse(event.occurred_at));
        matches.into_iter().take(INBOX_LIMIT).cloned().collect()
    }
    fn inbox(&mut self, ui: &mut Ui) {
        let p = self.palette();
        ui.horizontal(|ui| {
            let search_width = (ui.available_width() - 72. - ui.spacing().item_spacing.x).max(80.);
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
            let filter = ui.add_sized(
                [72., 28.],
                egui::Button::new("Filter").selected(self.unread_only || self.kind.is_some()),
            );
            egui::Popup::menu(&filter)
                .id(filter_popup_id())
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .width(230.)
                .show(|ui| {
                    ui.checkbox(&mut self.unread_only, "Ungelesen");
                    ui.separator();
                    ui.selectable_value(&mut self.kind, None, "Alle Typen");
                    for kind in [Kind::Request, Kind::Issue, Kind::Review, Kind::Mention] {
                        ui.selectable_value(&mut self.kind, Some(kind), kind.label());
                    }
                    ui.separator();
                    if ui
                        .add_enabled(
                            self.events.iter().any(|event| event.unread),
                            egui::Button::new("Alle als gelesen markieren"),
                        )
                        .on_hover_text("Markiert den gesamten lokalen Verlauf als gelesen.")
                        .clicked()
                    {
                        self.read_all();
                        ui.close();
                    }
                });
        });
        ui.add_space(2.);
        let visible = self.visible_events();
        if visible.is_empty() {
            ui.add_space(32.);
            ui.vertical_centered(|ui| {
                label(
                    ui,
                    if !self.search.is_empty() || self.kind.is_some() {
                        "Keine Treffer"
                    } else if self.unread_only {
                        "Keine ungelesenen Benachrichtigungen"
                    } else {
                        "Keine Benachrichtigungen"
                    },
                    13.,
                    p.muted,
                );
            });
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt(("inbox-scroll", self.kind, self.unread_only, &self.search))
            .max_height(ui.available_height().max(40.))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                for event in visible {
                    let card = event_card(ui, &event, p);
                    if card.github.clicked() {
                        self.open_event(&event);
                    } else if card.row.clicked() {
                        self.read_event(&event);
                    }
                }
            });
    }
    fn read_all(&mut self) {
        let result = self.store.as_ref().map_or(Ok(()), |store| store.read_all());
        if result.is_ok() {
            for event in &mut self.events {
                event.unread = false;
            }
        }
        self.report(result, "Alle als gelesen markiert.");
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
                unread: self.events.iter().filter(|e| e.unread).count(),
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
            egui::Popup::close_all(root.ctx());
        }
        if root.input(|i| i.key_pressed(egui::Key::Escape)) {
            if !egui::Popup::is_any_open(root.ctx()) {
                self.page = Page::Inbox;
                self.message = None;
            }
            egui::Popup::close_all(root.ctx());
        }
        let p = self.palette();
        let rect = root.max_rect();
        root.painter().rect_filled(rect, 0, p.bg);
        let content = rect.shrink2(vec2(12., 10.));
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
        } else {
            self.inbox(&mut ui);
        }
        Toast::show(&mut self.message, root.ctx(), p);
    }
}
impl Drop for HushApp {
    fn drop(&mut self) {
        self.cancel_oauth();
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

fn filter_popup_id() -> egui::Id {
    egui::Id::new("inbox-filters")
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

struct EventCardResponse {
    row: egui::Response,
    github: egui::Response,
}

fn event_card(ui: &mut Ui, event: &Event, p: Palette) -> EventCardResponse {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.), Sense::hover());
    let response = ui.interact(r, ui.id().with(&event.id), Sense::click());
    let github_rect = Rect::from_min_size(r.right_top() + vec2(-34., 4.), vec2(28., 28.));
    // Register the action every frame so keyboard users can reach it without hovering.
    let github = ui.interact(github_rect, response.id.with("github"), Sense::click());
    let show_github = response.contains_pointer() || response.has_focus() || github.has_focus();
    if show_github {
        ui.painter().rect_filled(r, 4, p.card);
    }
    if response.has_focus() {
        ui.painter().rect_stroke(
            r.shrink(1.),
            4,
            Stroke::new(1., p.accent),
            StrokeKind::Inside,
        );
    }
    ui.painter().line_segment(
        [r.left_bottom(), r.right_bottom()],
        Stroke::new(1., p.border),
    );
    icons::paint(
        ui.painter(),
        Rect::from_min_size(r.min + vec2(6., 9.), vec2(16., 16.)),
        event.kind.into(),
        p.kind(event.kind),
    );
    paint_elided(
        ui,
        r.min + vec2(30., 8.),
        &event.title,
        13.,
        if event.unread { p.text } else { p.muted },
        r.width() - 70.,
    );
    paint_elided(
        ui,
        r.min + vec2(30., 31.),
        &format!("{} · @{}", event.repository, event.actor),
        11.,
        p.faint,
        r.width() - 110.,
    );
    ui.painter().text(
        r.right_top() + vec2(-6., 32.),
        Align2::RIGHT_TOP,
        age(event),
        FontId::proportional(10.5),
        p.faint,
    );
    if event.unread {
        ui.painter()
            .circle_filled(pos2(r.left() + 14., r.top() + 37.), 2.5, p.accent);
    }
    if response.clicked() {
        response.request_focus();
    }
    if github.clicked() {
        github.request_focus();
    }
    if show_github {
        if github.hovered() || github.has_focus() {
            ui.painter().rect_filled(github_rect, 4, p.hover);
        }
        if github.has_focus() {
            ui.painter().rect_stroke(
                github_rect,
                4,
                Stroke::new(1., p.accent),
                StrokeKind::Inside,
            );
        }
        icons::paint(
            ui.painter(),
            github_rect.shrink(6.),
            Icon::ExternalLink,
            p.text,
        );
    }
    github.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!("{} – auf GitHub öffnen", event.title),
        )
    });
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            true,
            format!(
                "{}: {} – als gelesen markieren",
                event.kind.short(),
                event.title
            ),
        )
    });
    let row = response
        .on_hover_text(format!(
            "{}\n{}\n{} · @{}\nAls gelesen markieren",
            event.kind.short(),
            event.title,
            event.repository,
            event.actor
        ))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    EventCardResponse {
        row,
        github: github
            .on_hover_text("Auf GitHub öffnen")
            .on_hover_cursor(egui::CursorIcon::PointingHand),
    }
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
