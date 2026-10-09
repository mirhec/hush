use super::*;

impl HushApp {
    pub(super) fn settings(&mut self, ui: &mut Ui) {
        let p = self.palette();
        let l = self.language();
        ui.horizontal(|ui| {
            label(ui, l.text("Sprache"), 12., p.muted);
            let mut language = self.config.language;
            let selected = if language == Language::System {
                l.format("System ({language})", &[("language", l.native_name())])
            } else {
                language.native_name().to_owned()
            };
            egui::ComboBox::from_id_salt("language")
                .selected_text(selected)
                .height(280.)
                .width((ui.available_width() - 8.).min(210.))
                .show_ui(ui, |ui| {
                    for option in Language::ALL {
                        let title = if option == Language::System {
                            l.text("Systemsprache")
                        } else {
                            option.native_name()
                        };
                        ui.selectable_value(&mut language, option, title);
                    }
                })
                .response
                .on_hover_text(l.text("Sprache und Darstellung werden automatisch gespeichert."));
            if language != self.config.language {
                self.save_appearance(language, self.light, ui.ctx());
            }
        });
        ui.add_space(4.);
        ui.horizontal_wrapped(|ui| {
            for (tab, title) in [
                (SettingsTab::Notifications, l.text("Benachrichtigungen")),
                (SettingsTab::Account, l.text("Konto")),
                (SettingsTab::Diagnostics, l.text("Diagnose")),
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
                if !self.demo && ui.button(l.text("Hush beenden")).clicked() {
                    self.quit(ui.ctx());
                }
                label(
                    ui,
                    concat!("Hush ", env!("CARGO_PKG_VERSION")),
                    11.,
                    p.faint,
                );
            } else {
                if primary(ui, l.text("Speichern"), p).clicked() {
                    self.save_settings();
                }
                if self.settings_tab == SettingsTab::Notifications
                    && ui.button(l.text("Test-Benachrichtigung")).clicked()
                {
                    self.start_service(ui.ctx());
                    let result = self
                        .store
                        .as_ref()
                        .map_or(Ok(()), |store| store.put("test_notification", "1"));
                    self.report(
                        result,
                        if self.demo {
                            l.text("Demo: keine System-Benachrichtigung gesendet.")
                        } else {
                            l.text("Test-Benachrichtigung angefragt.")
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
        let l = self.language();
        section(ui, l.text("Ereignisse"), p);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            for (kind, help) in [
                (
                    Kind::Request,
                    l.text("Zuweisungen und Review-Anfragen an dich oder dein Team."),
                ),
                (
                    Kind::Issue,
                    l.text("Neue Issues in den unten eingetragenen Repositories."),
                ),
                (Kind::Review, l.text("Reviews zu deinen Pull Requests.")),
                (Kind::Mention, l.text("Neue @Erwähnungen in Kommentaren.")),
            ] {
                setting_toggle(
                    ui,
                    l.text(kind.label()),
                    help,
                    self.draft.rules.get_mut(kind),
                    p,
                    Some(kind.into()),
                );
            }
        });
        section(ui, l.text("Issue-Repositories"), p);
        ui.add(
            egui::TextEdit::multiline(&mut self.repos_text)
                .hint_text("organisation/repository")
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .margin(vec2(8., 6.)),
        );
        label(ui, l.text("Ein Repository pro Zeile"), 11., p.faint);
        if !self.demo && !self.config.repositories.is_empty() {
            egui::CollapsingHeader::new(l.text("Repository-Zugriff"))
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
                                    l.text("Kein Zugriff")
                                } else if check.is_some() {
                                    l.text("Verbunden")
                                } else {
                                    l.text("Noch nicht geprüft")
                                })
                                .size(11.)
                                .color(if error.is_some() { p.danger } else { p.muted }),
                            )
                            .on_hover_text(
                                l.message(error.unwrap_or("Status des letzten Issue-Abrufs.")),
                            );
                        });
                    }
                });
        }
    }

    fn delivery_preferences(&mut self, ui: &mut Ui) {
        let p = self.palette();
        let l = self.language();
        section(ui, l.text("Desktop"), p);
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            setting_toggle(ui, l.text("Desktop-Benachrichtigungen"), l.text("Auch bei geschlossenem Fenster."), &mut self.draft.desktop_notifications, p, None);
            setting_toggle(ui, l.text("Inhalte im Banner anzeigen"), l.text("Titel, Personen und Repository-Namen. Ausgeschaltet bleiben Inhalte im Banner privat."), &mut self.draft.show_preview, p, None);
        });
        section(ui, l.text("Aktualisierung"), p);
        ui.horizontal_wrapped(|ui| {
            for (seconds, title) in [
                (60, l.text("1 Min.")),
                (120, l.text("2 Min.")),
                (300, l.text("5 Min.")),
            ] {
                ui.selectable_value(&mut self.draft.interval_secs, seconds, title)
                    .on_hover_text(l.text(
                        "Bei GitHub-Limits verlängert sich das Abfrageintervall automatisch.",
                    ));
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(if self.config.paused() {
                    l.text("Fortsetzen")
                } else {
                    l.text("30 Min. pausieren")
                })
                .on_hover_text(l.text("Desktop-Benachrichtigungen vorübergehend pausieren"))
                .clicked()
            {
                self.pause();
            }
            if self.config.paused() {
                label(ui, l.text("Pausiert"), 11., p.amber);
            }
        });
    }

    fn account_settings(&mut self, ui: &mut Ui) {
        let p = self.palette();
        let l = self.language();
        if self.demo {
            section(ui, l.text("GitHub-Konto"), p);
            label(ui, l.text("@alex · Demokonto"), 14., p.text);
            label(
                ui,
                l.text("Die Demo verwendet keine Zugangsdaten."),
                12.,
                p.muted,
            );
            return;
        }
        section(ui, l.text("GitHub-Konto"), p);
        if !self.config.login.is_empty() {
            label(
                ui,
                l.message(&format!("Verbunden als @{}", self.config.login)),
                13.,
                p.accent,
            );
        }
        self.oauth_settings(ui);
        if !self.config.oauth {
            ui.add_space(8.);
            egui::CollapsingHeader::new(l.text("Manuelle Tokens"))
                .default_open(!self.config.login.is_empty())
                .show(ui, |ui| {
                    ui.add_enabled_ui(self.job.is_none() && self.oauth_login.is_none(), |ui| {
                        self.manual_token_settings(ui);
                    });
                });
        }
        ui.add_space(8.);
        egui::CollapsingHeader::new(l.text("Manuelle Team-Liste")).show(ui, |ui| {
            label(
                ui,
                l.text("Ohne read:org hier organisation/team-slug eintragen."),
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
            ui.add_enabled_ui(self.job.is_none() && self.oauth_login.is_none(), |ui| {
                if ui.small_button(l.text("Konto trennen …")).clicked() {
                    self.confirm_disconnect = true;
                }
                if self.confirm_disconnect {
                    label(
                        ui,
                        l.text(
                            "Konto trennen, gespeicherte Zugangsdaten und lokale Daten löschen?",
                        ),
                        12.,
                        p.danger,
                    );
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(l.text("Trennen und löschen")).clicked() {
                            self.account_job(true);
                            self.confirm_disconnect = false;
                        }
                        if ui.button(l.text("Abbrechen")).clicked() {
                            self.confirm_disconnect = false;
                        }
                    });
                }
            });
        }
    }

    fn oauth_settings(&mut self, ui: &mut Ui) {
        let p = self.palette();
        let l = self.language();
        if let Some(login) = &self.oauth_login {
            let user_code = login.user_code.clone();
            let verification_uri = login.verification_uri.clone();
            let expires_at = login.expires_at;
            if let Some(code) = user_code {
                label(ui, l.text("Diesen Code auf GitHub eingeben:"), 12., p.muted);
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&code).monospace().size(21.).color(p.text));
                    if ui.button(l.text("Kopieren")).clicked() {
                        ui.ctx().copy_text(code.clone());
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    if let Some(uri) = &verification_uri
                        && ui.button(l.text("GitHub öffnen ↗")).clicked()
                    {
                        self.open(uri);
                    }
                    if ui.button(l.text("Abbrechen")).clicked() {
                        self.cancel_oauth();
                    }
                });
                ui.horizontal_wrapped(|ui| {
                    ui.spinner();
                    label(ui, l.text("Warte auf Bestätigung …"), 12., p.muted);
                    if let Some(expires_at) = expires_at {
                        let seconds = expires_at
                            .saturating_duration_since(Instant::now())
                            .as_secs();
                        label(
                            ui,
                            l.message(&format!("{}:{:02} Min.", seconds / 60, seconds % 60)),
                            11.,
                            p.faint,
                        );
                    }
                });
            } else {
                ui.horizontal_wrapped(|ui| {
                    ui.spinner();
                    label(
                        ui,
                        l.text("GitHub-Anmeldung wird vorbereitet …"),
                        12.,
                        p.muted,
                    );
                    if ui.button(l.text("Abbrechen")).clicked() {
                        self.cancel_oauth();
                    }
                });
            }
            return;
        }
        ui.add_space(6.);
        ui.horizontal_wrapped(|ui| {
            let enabled = self.job.is_none() && crate::oauth::client_id().is_some();
            if ui
                .add_enabled_ui(enabled, |ui| {
                    primary(
                        ui,
                        if self.config.oauth {
                            l.text("Erneut anmelden")
                        } else {
                            l.text("Mit GitHub anmelden")
                        },
                        p,
                    )
                })
                .inner
                .clicked()
            {
                self.start_oauth();
            }
            if self.job.is_some() {
                ui.spinner();
            }
        });
        if crate::oauth::client_id().is_none() {
            label(
                ui,
                l.text("GitHub-Anmeldung ist in diesem Build nicht eingerichtet."),
                11.5,
                p.muted,
            );
        } else {
            label(
                ui,
                l.text("Für private Repositories umfasst die GitHub-Freigabe auch Schreibrechte. Hush liest ausschließlich."),
                11.5,
                p.muted,
            );
        }
    }

    fn manual_token_settings(&mut self, ui: &mut Ui) {
        let p = self.palette();
        let l = self.language();
        label(ui, l.text("Benachrichtigungs-Token"), 13., p.text);
        ui.add(
            egui::TextEdit::singleline(&mut self.token)
                .password(true)
                .hint_text(if self.config.login.is_empty() {
                    l.text("Classic Token")
                } else {
                    l.text("Gespeichert · leer lassen zum Behalten")
                })
                .desired_width(f32::INFINITY)
                .margin(vec2(8., 6.)),
        );
        label(
            ui,
            l.text("Classic Token: notifications, optional read:org für Teams."),
            11.5,
            p.muted,
        );
        ui.add_space(6.);
        label(
            ui,
            l.text("Detail-Token für private Repositories"),
            13.,
            p.text,
        );
        ui.add(
            egui::TextEdit::singleline(&mut self.details_token)
                .password(true)
                .hint_text(if self.config.has_detail_token {
                    l.text("Gespeichert · leer lassen zum Behalten")
                } else {
                    l.text("Fine-grained Token")
                })
                .desired_width(f32::INFINITY)
                .margin(vec2(8., 6.)),
        );
        label(
            ui,
            l.text("Repos auswählen; Issues und Pull requests: Read-only. Bei Bedarf Discussions: Read-only. Eine Organisationsfreigabe kann erforderlich sein."),
            11.5,
            p.muted,
        );
        ui.horizontal_wrapped(|ui| {
            if ui
                .small_button(l.text("Detail-Token erstellen ↗"))
                .clicked()
            {
                self.open("https://github.com/settings/personal-access-tokens/new");
            }
            if ui.small_button(l.text("Token-Einstellungen ↗")).clicked() {
                self.open("https://github.com/settings/tokens");
            }
        });
        ui.add_space(4.);
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(
                    !self.token.trim().is_empty()
                        || (!self.config.login.is_empty() && !self.details_token.trim().is_empty()),
                    egui::Button::new(if self.config.login.is_empty() {
                        l.text("Verbinden")
                    } else {
                        l.text("Tokens speichern")
                    })
                    .fill(p.hover),
                )
                .clicked()
            {
                self.account_job(false);
            }
            label(
                ui,
                l.text("Speicherung im System-Schlüsselbund"),
                11.,
                p.faint,
            );
        });
    }

    fn diagnostics(&mut self, ui: &mut Ui) {
        let p = self.palette();
        let l = self.language();
        section(ui, l.text("Dienst"), p);
        if !self.demo {
            label(
                ui,
                l.message(&format!(
                    "{} · {} API-Aufrufe · {} Teams",
                    l.message(&self.status.phase),
                    self.status.requests_last_cycle,
                    self.status.team_count
                )),
                12.,
                p.muted,
            );
            if let Some(remaining) = self.status.remaining {
                label(
                    ui,
                    l.message(&format!("API-Restbudget: {remaining}")),
                    11.,
                    p.faint,
                );
            }
            for warning in &self.status.warnings {
                label(ui, l.message(warning), 12., p.amber);
            }
            for error in [&self.status.notification_error, &self.status.service_error]
                .into_iter()
                .flatten()
            {
                label(ui, l.message(error), 12., p.danger);
            }
            if let Some(paths) = &self.paths {
                label(
                    ui,
                    l.message(&format!("Protokoll: {}", paths.log_path().display())),
                    11.,
                    p.faint,
                );
            }
            ui.horizontal_wrapped(|ui| {
                if ui.button(l.text("Dienst starten")).clicked() {
                    self.start_service(ui.ctx());
                }
                if ui.button(l.text("Dienst beenden")).clicked() {
                    let result = self
                        .store
                        .as_ref()
                        .map_or(Ok(()), |store| store.request_stop());
                    self.report(result, l.text("Hintergrunddienst wird beendet …"));
                }
            });
            label(
                ui,
                if self.tray_available {
                    l.text("Tray verfügbar")
                } else {
                    l.text("Tray nicht verfügbar · Hush über den Launcher öffnen")
                },
                12.,
                p.muted,
            );
        } else {
            label(ui, l.text("Demo · kein Hintergrunddienst"), 12., p.muted);
        }
        if ui.button(l.text("Jetzt aktualisieren")).clicked() {
            self.refresh();
        }
        section(ui, l.text("Lokale Daten"), p);
        label(
            ui,
            l.text("Bis zu 500 Ereignisse / 30 Tage. Inhaltsdaten sind lokal unverschlüsselt; Tokens liegen im Schlüsselbund."),
            12.,
            p.muted,
        );
        if let Some(paths) = &self.paths {
            label(ui, paths.root.display().to_string(), 11., p.faint);
        }
        if ui.button(l.text("Verlauf leeren …")).clicked() {
            self.confirm_clear = true;
        }
        if self.confirm_clear {
            ui.horizontal_wrapped(|ui| {
                if ui.button(l.text("Verlauf wirklich leeren")).clicked() {
                    let result = self
                        .store
                        .as_ref()
                        .map_or(Ok(()), |store| store.clear_history());
                    if result.is_ok() {
                        self.events.clear();
                    }
                    self.report(
                        result,
                        l.text("Verlauf geleert. Alte Ereignisse werden nicht erneut gemeldet."),
                    );
                    self.confirm_clear = false;
                }
                if ui.button(l.text("Abbrechen")).clicked() {
                    self.confirm_clear = false;
                }
            });
        }
        section(ui, l.text("Unterstützte Ereignisse"), p);
        label(
            ui,
            l.text("Erwähnungen in Issues, Pull Requests, Reviews, Commits und Repository-Discussions benötigen Zugriff auf das jeweilige Thema. Gists, Organisations-Discussions, Projects-Boards und GitHub Enterprise sind nicht unterstützt."),
            12.,
            p.muted,
        );
        label(
            ui,
            l.text("Der erste Import erfolgt ohne Desktop-Benachrichtigungen."),
            12.,
            p.muted,
        );
    }
}
