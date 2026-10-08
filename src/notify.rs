//! System notification backend. No curl, shell, remote icons, urgent alerts or notification relay.
use crate::model::{APP_ID, Config, Event};
use anyhow::Result;
use notify_rust::{Notification, Timeout};

fn safe_text(text: &str, limit: usize) -> String {
    let s: String=text.chars().filter(|c| !c.is_control() || *c=='\n').take(limit).collect();
    #[cfg(all(unix,not(target_os="macos")))]
    { s.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;") }
    #[cfg(any(target_os="windows",target_os="macos"))]
    { s }
}
fn show(title: &str, body: &str) -> Result<()> {
    #[cfg(target_os="macos")]
    {
        static APP: std::sync::OnceLock<Result<(),String>>=std::sync::OnceLock::new();
        APP.get_or_init(||notify_rust::set_application(APP_ID).map(|_|()).map_err(|_|"macOS-App-Identität konnte nicht registriert werden. Hush bitte aus dem .app-Bundle starten.".to_owned()))
            .as_ref().map_err(|e|anyhow::anyhow!("{e}"))?;
    }
    let mut notification=Notification::new();
    notification.appname("Hush").summary(&safe_text(title,120)).body(&safe_text(body,240)).timeout(Timeout::Milliseconds(7000));
    #[cfg(target_os="linux")]
    { notification.icon(APP_ID).hint(notify_rust::Hint::DesktopEntry(APP_ID.into())); }
    #[cfg(target_os="windows")]
    { notification.app_id(APP_ID); }
    notification.show().map_err(|_|anyhow::anyhow!("Systembenachrichtigung konnte nicht zugestellt werden. Bitte Betriebssystem-Berechtigung bzw. D-Bus-Benachrichtigungsdienst prüfen."))?;
    Ok(())
}
pub fn test() -> Result<()> { show("Hush: Test-Benachrichtigung","Desktop-Benachrichtigungen sind verfügbar.") }
pub fn deliver(events: &[Event], config: &Config) -> Result<()> {
    if events.is_empty() { return Ok(()); }
    if events.len()>2 {
        return show(&format!("{} neue Benachrichtigungen",events.len()),"Details in Hush öffnen.");
    }
    for event in events {
        if config.show_preview {
            show(event.kind.short(),&format!("{}\n{} · {}",event.title,event.repository,event.actor))?;
        } else {
            show(event.kind.short(),"Details in Hush öffnen.")?;
        }
    }
    Ok(())
}
