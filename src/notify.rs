//! System notification backend. No curl, shell, remote icons, urgent alerts or notification relay.
use crate::{
    i18n::Language,
    model::{APP_ID, Config, Event},
};
use anyhow::Result;
use notify_rust::{Notification, Timeout};

fn safe_text(text: &str, limit: usize) -> String {
    let s: String = text
        .chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .take(limit)
        .collect();
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        s
    }
}
fn show(title: &str, body: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        static APP: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
        APP.get_or_init(||notify_rust::set_application(APP_ID).map(|_|()).map_err(|_|"macOS-App-Identität konnte nicht registriert werden. Hush bitte aus dem .app-Bundle starten.".to_owned()))
            .as_ref().map_err(|e|anyhow::anyhow!("{e}"))?;
    }
    let mut notification = Notification::new();
    notification
        .appname("Hush")
        .summary(&safe_text(title, 120))
        .body(&safe_text(body, 240))
        .timeout(Timeout::Milliseconds(7000));
    #[cfg(target_os = "linux")]
    {
        notification
            .icon(APP_ID)
            .hint(notify_rust::Hint::DesktopEntry(APP_ID.into()));
    }
    #[cfg(target_os = "windows")]
    {
        notification.app_id(APP_ID);
    }
    notification.show().map_err(|_|anyhow::anyhow!("Systembenachrichtigung konnte nicht zugestellt werden. Bitte Betriebssystem-Berechtigung bzw. D-Bus-Benachrichtigungsdienst prüfen."))?;
    Ok(())
}
pub fn test() -> Result<()> {
    test_with_language(Language::System)
}

pub fn test_with_language(language: Language) -> Result<()> {
    let language = language.resolved();
    show(
        language.text("Hush: Test-Benachrichtigung"),
        language.text("Desktop-Benachrichtigungen sind verfügbar."),
    )
}

fn messages(events: &[Event], config: &Config) -> Vec<(String, String)> {
    let language = config.language.resolved();
    if events.len() > 2 {
        return vec![(
            language.format(
                "{} neue Benachrichtigungen",
                &[("0", &events.len().to_string())],
            ),
            language.text("Details in Hush öffnen.").to_owned(),
        )];
    }
    events
        .iter()
        .map(|event| {
            let body = if config.show_preview {
                format!("{}\n{} · {}", event.title, event.repository, event.actor)
            } else {
                language.text("Details in Hush öffnen.").to_owned()
            };
            (language.text(event.kind.short()).to_owned(), body)
        })
        .collect()
}

pub fn deliver(events: &[Event], config: &Config) -> Result<()> {
    for (title, body) in messages(events, config) {
        show(&title, &body)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;

    fn event() -> Event {
        Event {
            id: "1".into(),
            kind: Kind::Issue,
            title: "Einstellungen gespeichert. {count}".into(),
            repository: "example/private".into(),
            actor: "alice".into(),
            detail: "Confidential detail".into(),
            url: "https://github.com/example/private/issues/1".into(),
            occurred_at: chrono::Utc::now(),
            unread: true,
        }
    }

    #[test]
    fn localized_notifications_preserve_github_content_and_preview_privacy() {
        let event = event();
        for language in Language::ALL
            .into_iter()
            .filter(|language| *language != Language::System)
        {
            let mut config = Config {
                language,
                ..Config::default()
            };
            assert_eq!(messages(&[], &config), Vec::<(String, String)>::new());
            assert_eq!(
                messages(std::slice::from_ref(&event), &config),
                vec![(
                    language.text("Neues Issue").into(),
                    language.text("Details in Hush öffnen.").into()
                )]
            );
            config.show_preview = true;
            assert_eq!(
                messages(std::slice::from_ref(&event), &config),
                vec![(
                    language.text("Neues Issue").into(),
                    "Einstellungen gespeichert. {count}\nexample/private · alice".into()
                )]
            );
            let grouped = messages(&[event.clone(), event.clone(), event.clone()], &config);
            assert_eq!(
                grouped,
                vec![(
                    language.format("{} neue Benachrichtigungen", &[("0", "3")]),
                    language.text("Details in Hush öffnen.").into()
                )]
            );
        }
    }
}
