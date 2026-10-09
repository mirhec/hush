//! Native OS credential storage. No plaintext fallback, command line or environment variable tokens.
use anyhow::{Context, Result};
use zeroize::Zeroizing;
const SERVICE: &str = "io.hush.github";
#[derive(Clone, Copy)]
pub enum Slot {
    Notifications,
    Details,
    OAuth,
}
impl Slot {
    fn key(self) -> &'static str {
        match self {
            Self::Notifications => "github-notifications",
            Self::Details => "github-readonly-details",
            Self::OAuth => "github-oauth",
        }
    }
}
pub fn read(slot: Slot) -> Result<Option<Zeroizing<String>>> {
    let entry =
        keyring::Entry::new(SERVICE, slot.key()).context("Schlüsselbund nicht verfügbar.")?;
    match entry.get_password() {
        Ok(s) => Ok(Some(Zeroizing::new(s))),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => anyhow::bail!(
            "System-Schlüsselbund ist gesperrt oder nicht verfügbar. Unter Linux wird ein entsperrter Secret Service benötigt."
        ),
    }
}
pub fn save(slot: Slot, token: &str) -> Result<()> {
    keyring::Entry::new(SERVICE,slot.key())?.set_password(token)
        .map_err(|_| anyhow::anyhow!("Token konnte nicht im System-Schlüsselbund gespeichert werden. Es wird keine Klartext-Kopie angelegt."))
}
pub fn delete(slot: Slot) -> Result<()> {
    match keyring::Entry::new(SERVICE, slot.key())?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => anyhow::bail!(
            "Token konnte nicht aus dem Schlüsselbund gelöscht werden. Bitte dort manuell entfernen und gegebenenfalls auf GitHub widerrufen."
        ),
    }
}
