//! Explicit per-user login startup. Reading status never changes OS settings.
use anyhow::{Context, Result};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(not(target_os = "linux"), test))]
mod native;
#[cfg(target_os = "linux")]
mod portal;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    /// None means that the system API cannot report the current setting.
    pub enabled: Option<bool>,
    pub portal: bool,
}

pub fn status() -> Result<Status> {
    #[cfg(target_os = "linux")]
    if linux::in_flatpak() {
        // Background has no read/query method. Never invent an "off" state or
        // prompt for permission merely because the settings page was opened.
        return Ok(Status {
            enabled: None,
            portal: true,
        });
    }
    #[cfg(target_os = "linux")]
    let enabled = linux::status();
    #[cfg(not(target_os = "linux"))]
    let enabled = native::status();
    Ok(Status {
        enabled: Some(enabled.context("Autostart konnte nicht geprüft werden.")?),
        portal: false,
    })
}

/// Called only in response to an explicit user action, on a worker thread.
pub fn set_enabled(enabled: bool, reason: &str) -> Result<Status> {
    #[cfg(target_os = "linux")]
    if linux::in_flatpak() {
        let confirmed =
            portal::request(enabled, reason).context("Autostart konnte nicht geändert werden.")?;
        return Ok(Status {
            enabled: Some(confirmed),
            portal: true,
        });
    }
    #[cfg(target_os = "linux")]
    let confirmed = linux::set_enabled(enabled);
    #[cfg(not(target_os = "linux"))]
    let confirmed = {
        let _ = reason;
        native::set_enabled(enabled)
    };
    Ok(Status {
        enabled: Some(confirmed.context("Autostart konnte nicht geändert werden.")?),
        portal: false,
    })
}
