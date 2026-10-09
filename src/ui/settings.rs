use super::*;

impl HushApp {
    pub(super) fn settings(&mut self, ui: &mut Ui) {
        let p = self.palette();
        ui.horizontal(|ui| {
            for (tab, title) in [
                (SettingsTab::Notifications, "Benachrichtigungen"),
                (SettingsTab::Account, "Konto"),
                (SettingsTab::Diagnostics, "Diagnose"),
            ] {
                ui.selectable_value(&mut self.settings_tab, tab, title);
            }
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt(("settings-scroll", self.settings_tab as u8))
            .max_height((ui.available_height() - 44.).max(80.))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_max_width(ui.available_width().min(900.));
                match self.settings_tab {
                    SettingsTab::Notifications => self.notification_settings(ui),
                    SettingsTab::Account => self.account_settings(ui),
                    SettingsTab::Diagnostics => self.diagnostics(ui),
                }
            });
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            if self.settings_tab == SettingsTab::Diagnostics {
                if !self.demo && ui.button("Hush beenden").clicked() {
                    self.quit(ui.ctx());
                }
                label(
                    ui,
                    concat!("Hush ", env!("CARGO_PKG_VERSION")),
                    11.,
                    p.faint,
                );
            } else {
                if primary(ui, "Speichern", p).clicked() {
                    self.save_settings();
                }
                if self.settings_tab == SettingsTab::Notifications
                    && ui.button("Test-Benachrichtigung").clicked()
                {
                    self.start_service(ui.ctx());
                    let result = self
                        .store
                        .as_ref()
                        .map_or(Ok(()), |store| store.put("test_notification", "1"));
                    self.report(
                        result,
                        if self.demo {
                            "Demo: keine System-Benachrichtigung gesendet."
                        } else {
                            "Test-Benachrichtigung angefragt."
                        },
                    );
                }
            }
        });
    }

    fn notification_settings(&mut self, ui: &mut Ui) {
        if ui.available_width() >= 520. {
            ui.columns(2, |columns| {
                self.event_preferences(&mut columns[0]);
                self.delivery_preferences(&mut columns[1]);
            });
        } else {
            self.event_preferences(ui);
            self.delivery_preferences(ui);
        }
    }

    fn event_preferences(&mut self, ui: &mut Ui) {
        let p = self.palette();
        section(ui, "Ereignisse", p);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            for (kind, help) in [
                (
                    Kind::Request,
                    "Zuweisungen und Review-Anfragen an dich oder dein Team.",
                ),
                (
                    Kind::Issue,
                    "Neue Issues in den unten eingetragenen Repositories.",
                ),
                (Kind::Review, "Reviews zu deinen Pull Requests."),
                (Kind::Mention, "Neue @Erwähnungen in Kommentaren."),
            ] {
                setting_toggle(
                    ui,
                    kind.label(),
                    help,
                    self.draft.rules.get_mut(kind),
                    p,
                    Some(kind.into()),
                );
            }
        });
        section(ui, "Issue-Repositories", p);
        ui.add(
            egui::TextEdit::multiline(&mut self.repos_text)
                .hint_text("organisation/repository")
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .margin(vec2(8., 6.)),
        );
        label(ui, "Ein Repository pro Zeile", 11., p.faint);
        if !self.demo && !self.config.repositories.is_empty() {
            egui::CollapsingHeader::new("Repository-Zugriff")
                .default_open(
                    self.status
                        .repositories
                        .iter()
                        .any(|check| check.error.is_some())
                        || !self.status.warnings.is_empty(),
                )
                .show(ui, |ui| {
                    for repo in &self.config.repositories {
                        let name = repo.to_string();
                        let check = self
                            .status
                            .repositories
                            .iter()
                            .find(|check| check.repository == name);
                        let error = check.and_then(|check| check.error.as_deref()).or_else(|| {
                            self.status
                                .warnings
                                .iter()
                                .find(|warning| warning.starts_with(&format!("{name}:")))
                                .map(String::as_str)
                        });
                        ui.horizontal_wrapped(|ui| {
                            label(ui, name, 12., p.text);
                            ui.label(
                                RichText::new(if error.is_some() {
                                    "Kein Zugriff"
                                } else if check.is_some() {
                                    "Verbunden"
                                } else {
                                    "Noch nicht geprüft"
                                })
                                .size(11.)
                                .color(if error.is_some() { p.danger } else { p.muted }),
                            )
                            .on_hover_text(error.unwrap_or("Status des letzten Issue-Abrufs."));
                        });
                    }
                });
        }
    }

    fn delivery_preferences(&mut self, ui: &mut Ui) {
        let p = self.palette();
        section(ui, "Desktop", p);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            setting_toggle(ui, "Desktop-Benachrichtigungen", "Auch bei geschlossenem Fenster.", &mut self.draft.desktop_notifications, p, None);
            setting_toggle(ui, "Inhalte im Banner anzeigen", "Titel, Personen und Repository-Namen. Ausgeschaltet bleiben Inhalte im Banner privat.", &mut self.draft.show_preview, p, None);
        });
        section(ui, "Aktualisierung", p);
        ui.horizontal_wrapped(|ui| {
            for (seconds, title) in [(60, "1 Min."), (120, "2 Min."), (300, "5 Min.")] {
                ui.selectable_value(&mut self.draft.interval_secs, seconds, title)
                    .on_hover_text(
                        "Bei GitHub-Limits verlängert sich das Abfrageintervall automatisch.",
                    );
            }
        });
    }

    fn account_settings(&mut self, ui: &mut Ui) {
        let p = self.palette();
        if self.demo {
            section(ui, "GitHub-Konto", p);
            label(ui, "@alex · Demokonto", 14., p.text);
            label(ui, "Die Demo verwendet keine Zugangsdaten.", 12., p.muted);
            return;
        }
        section(
            ui,
            if self.config.login.is_empty() {
                "GitHub verbinden"
            } else {
                "Zugangsdaten"
            },
            p,
        );
        if !self.config.login.is_empty() {
            label(
                ui,
                format!("Verbunden als @{}", self.config.login),
                13.,
                p.accent,
            );
        }
        label(ui, "Benachrichtigungs-Token", 13., p.text);
        ui.add(
            egui::TextEdit::singleline(&mut self.token)
                .password(true)
                .hint_text(if self.config.login.is_empty() {
                    "Classic Token"
                } else {
                    "Gespeichert · leer lassen zum Behalten"
                })
                .desired_width(f32::INFINITY)
                .margin(vec2(8., 6.)),
        );
        label(
            ui,
            "Classic Token: notifications, optional read:org für Teams.",
            11.5,
            p.muted,
        );
        ui.add_space(6.);
        label(ui, "Detail-Token für private Repositories", 13., p.text);
        ui.add(
            egui::TextEdit::singleline(&mut self.details_token)
                .password(true)
                .hint_text(if self.config.has_detail_token {
                    "Gespeichert · leer lassen zum Behalten"
                } else {
                    "Fine-grained Token"
                })
                .desired_width(f32::INFINITY)
                .margin(vec2(8., 6.)),
        );
        label(
            ui,
            "Repos auswählen; Issues und Pull requests: Read-only. Bei Bedarf Discussions: Read-only. Eine Organisationsfreigabe kann erforderlich sein.",
            11.5,
            p.muted,
        );
        ui.horizontal_wrapped(|ui| {
            if ui.small_button("Detail-Token erstellen ↗").clicked() {
                self.open("https://github.com/settings/personal-access-tokens/new");
            }
            if ui.small_button("Token-Einstellungen ↗").clicked() {
                self.open("https://github.com/settings/tokens");
            }
        });
        ui.add_space(4.);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    self.job.is_none()
                        && (!self.token.trim().is_empty()
                            || (!self.config.login.is_empty()
                                && !self.details_token.trim().is_empty())),
                    egui::Button::new(if self.config.login.is_empty() {
                        "Verbinden"
                    } else {
                        "Tokens speichern"
                    })
                    .fill(p.hover),
                )
                .clicked()
            {
                self.account_job(false);
            }
            if self.job.is_some() {
                ui.spinner();
            }
            label(ui, "Speicherung im System-Schlüsselbund", 11., p.faint);
        });
        section(ui, "Teams", p);
        egui::CollapsingHeader::new("Manuelle Team-Liste").show(ui, |ui| {
            label(
                ui,
                "Ohne read:org hier organisation/team-slug eintragen.",
                12.,
                p.muted,
            );
            ui.add(
                egui::TextEdit::multiline(&mut self.teams_text)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY)
                    .hint_text("organisation/frontend"),
            );
        });
        if !self.config.login.is_empty() {
            ui.add_space(12.);
            if ui.small_button("Konto trennen …").clicked() {
                self.confirm_disconnect = true;
            }
            if self.confirm_disconnect {
                label(
                    ui,
                    "Konto trennen, beide gespeicherten Tokens und lokale Daten löschen?",
                    12.,
                    p.danger,
                );
                ui.horizontal(|ui| {
                    if ui.button("Trennen und löschen").clicked() {
                        self.account_job(true);
                        self.confirm_disconnect = false;
                    }
                    if ui.button("Abbrechen").clicked() {
                        self.confirm_disconnect = false;
                    }
                });
            }
        }
    }

    fn diagnostics(&mut self, ui: &mut Ui) {
        let p = self.palette();
        section(ui, "Dienst", p);
        if !self.demo {
            label(
                ui,
                format!(
                    "{} · {} API-Aufrufe · {} Teams",
                    self.status.phase, self.status.requests_last_cycle, self.status.team_count
                ),
                12.,
                p.muted,
            );
            if let Some(remaining) = self.status.remaining {
                label(ui, format!("API-Restbudget: {remaining}"), 11., p.faint);
            }
            for warning in &self.status.warnings {
                label(ui, warning, 12., p.amber);
            }
            for error in [&self.status.notification_error, &self.status.service_error]
                .into_iter()
                .flatten()
            {
                label(ui, error, 12., p.danger);
            }
            if let Some(paths) = &self.paths {
                label(
                    ui,
                    format!("Protokoll: {}", paths.log_path().display()),
                    11.,
                    p.faint,
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button("Dienst starten").clicked() {
                    self.start_service(ui.ctx());
                }
                if ui.button("Dienst beenden").clicked() {
                    let result = self
                        .store
                        .as_ref()
                        .map_or(Ok(()), |store| store.request_stop());
                    self.report(result, "Hintergrunddienst wird beendet …");
                }
            });
            label(
                ui,
                if self.tray_available {
                    "Tray verfügbar"
                } else {
                    "Tray nicht verfügbar · Hush über den Launcher öffnen"
                },
                12.,
                p.muted,
            );
        } else {
            label(ui, "Demo · kein Hintergrunddienst", 12., p.muted);
        }
        section(ui, "Lokale Daten", p);
        label(
            ui,
            "Bis zu 500 Ereignisse / 30 Tage. Inhaltsdaten sind lokal unverschlüsselt; Tokens liegen im Schlüsselbund.",
            12.,
            p.muted,
        );
        if let Some(paths) = &self.paths {
            label(ui, paths.root.display().to_string(), 11., p.faint);
        }
        if ui.button("Verlauf leeren …").clicked() {
            self.confirm_clear = true;
        }
        if self.confirm_clear {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Verlauf wirklich leeren").clicked() {
                    let result = self
                        .store
                        .as_ref()
                        .map_or(Ok(()), |store| store.clear_history());
                    if result.is_ok() {
                        self.events.clear();
                    }
                    self.report(
                        result,
                        "Verlauf geleert. Alte Ereignisse werden nicht erneut gemeldet.",
                    );
                    self.confirm_clear = false;
                }
                if ui.button("Abbrechen").clicked() {
                    self.confirm_clear = false;
                }
            });
        }
        section(ui, "Unterstützte Ereignisse", p);
        label(
            ui,
            "Erwähnungen in Issues, Pull Requests, Reviews, Commits und Repository-Discussions benötigen Zugriff auf das jeweilige Thema. Gists, Organisations-Discussions, Projects-Boards und GitHub Enterprise sind nicht unterstützt.",
            12.,
            p.muted,
        );
        label(
            ui,
            "Der erste Import erfolgt ohne Desktop-Benachrichtigungen.",
            12.,
            p.muted,
        );
    }
}
