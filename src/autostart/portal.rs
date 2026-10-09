//! Flatpak autostart through the Background portal; no host filesystem grants.
use anyhow::{Context, Result, bail, ensure};
use gio::glib::{self, ToVariant, Variant};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::Rc,
    time::{Duration, Instant},
};

const SERVICE: &str = "org.freedesktop.portal.Desktop";
const DESKTOP: &str = "/org/freedesktop/portal/desktop";
const REQUEST: &str = "org.freedesktop.portal.Request";

pub(super) fn request(enabled: bool, reason: &str) -> Result<bool> {
    let context = glib::MainContext::new();
    context.with_thread_default(|| request_inner(&context, enabled, reason))?
}

fn options(enabled: bool, reason: &str, token: &str) -> BTreeMap<&'static str, Variant> {
    BTreeMap::from([
        ("handle_token", token.to_variant()),
        ("reason", reason.to_variant()),
        ("autostart", enabled.to_variant()),
        // The portal wraps this command in the appropriate flatpak invocation.
        ("commandline", vec!["hush", "--tray"].to_variant()),
        ("dbus-activatable", false.to_variant()),
    ])
}

fn confirmed(response: &Variant, requested: bool) -> Result<bool> {
    let (code, results) = response
        .get::<(u32, BTreeMap<String, Variant>)>()
        .context("Invalid Background portal response")?;
    match code {
        0 => {}
        1 => bail!("Autostart-Anfrage abgebrochen."),
        _ => bail!("Autostart wurde vom System nicht bestätigt."),
    }
    let autostart = results
        .get("autostart")
        .and_then(Variant::get::<bool>)
        .context("Background portal did not report autostart permission")?;
    ensure!(
        autostart == requested,
        "Autostart wurde vom System nicht bestätigt."
    );
    if requested {
        ensure!(
            results.get("background").and_then(Variant::get::<bool>) == Some(true),
            "Autostart wurde vom System nicht bestätigt."
        );
    }
    Ok(autostart)
}

struct Connection(gio::DBusConnection);
impl Drop for Connection {
    fn drop(&mut self) {
        // Closing the private connection also ends any outstanding portal request.
        let _ = self.0.close_sync(None::<&gio::Cancellable>);
    }
}

fn request_inner(context: &glib::MainContext, enabled: bool, reason: &str) -> Result<bool> {
    let address =
        gio::dbus_address_get_for_bus_sync(gio::BusType::Session, None::<&gio::Cancellable>)?;
    let connection = Connection(gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        None::<&gio::Cancellable>,
    )?);
    let bus = &connection.0;
    bus.set_exit_on_close(false);
    let responses = Rc::new(RefCell::new(Vec::<(String, Variant)>::new()));
    let received = responses.clone();
    // Subscribe before requesting; an already-approved portal can respond before
    // RequestBackground returns. Filtering by the returned handle also supports
    // older portal implementations that choose their own request path.
    let subscription = bus.signal_subscribe(
        Some(SERVICE),
        Some(REQUEST),
        Some("Response"),
        None,
        None,
        gio::DBusSignalFlags::NONE,
        move |_, _, path, _, _, response| {
            let mut received = received.borrow_mut();
            if received.len() < 16 {
                received.push((path.to_owned(), response.clone()));
            }
        },
    );
    let result = (|| {
        let token = format!(
            "hush_{}_{:08x}{:08x}",
            std::process::id(),
            glib::random_int(),
            glib::random_int()
        );
        let reply = bus.call_sync(
            Some(SERVICE),
            DESKTOP,
            "org.freedesktop.portal.Background",
            "RequestBackground",
            Some(&("", options(enabled, reason, &token)).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            10_000,
            None::<&gio::Cancellable>,
        )?;
        let (handle,) = reply
            .get::<(glib::variant::ObjectPath,)>()
            .context("Invalid Background portal request handle")?;
        ensure!(
            handle
                .as_str()
                .starts_with("/org/freedesktop/portal/desktop/request/"),
            "Unexpected Background portal request path"
        );
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            while context.pending() {
                context.iteration(false);
            }
            for (path, response) in responses.borrow_mut().drain(..) {
                if path == handle.as_str() {
                    return confirmed(&response, enabled);
                }
            }
            if bus.is_closed() {
                bail!("Background portal connection closed");
            }
            if Instant::now() >= deadline {
                let _ = bus.call_sync(
                    Some(SERVICE),
                    handle.as_str(),
                    REQUEST,
                    "Close",
                    None,
                    None,
                    gio::DBusCallFlags::NONE,
                    2_000,
                    None::<&gio::Cancellable>,
                );
                bail!("Keine Antwort vom System. Bitte erneut versuchen.");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })();
    bus.signal_unsubscribe(subscription);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portal_requests_background_tray_start_without_host_escape() {
        for enabled in [false, true] {
            let options = options(enabled, "Run Hush in the tray at login", "test_handle");
            assert_eq!(("", &options).to_variant().type_().as_str(), "(sa{sv})");
            assert_eq!(options["autostart"].get::<bool>(), Some(enabled));
            assert_eq!(
                options["commandline"].get::<Vec<String>>(),
                Some(vec!["hush".into(), "--tray".into()])
            );
            assert_eq!(options["dbus-activatable"].get::<bool>(), Some(false));
        }
    }

    #[test]
    fn portal_success_requires_explicit_matching_permissions() {
        let results = |background: bool, autostart: bool| {
            BTreeMap::from([
                ("background", background.to_variant()),
                ("autostart", autostart.to_variant()),
            ])
        };
        assert!(confirmed(&(0u32, results(true, true)).to_variant(), true).unwrap());
        assert!(!confirmed(&(0u32, results(true, false)).to_variant(), false).unwrap());
        for code in [1u32, 2, 9] {
            assert!(confirmed(&(code, results(true, true)).to_variant(), true).is_err());
        }
        assert!(confirmed(&(0u32, results(false, true)).to_variant(), true).is_err());
        assert!(confirmed(&(0u32, results(true, false)).to_variant(), true).is_err());
        assert!(confirmed(&(0u32, results(true, true)).to_variant(), false).is_err());
        assert!(
            confirmed(
                &(0u32, BTreeMap::<String, Variant>::new()).to_variant(),
                true
            )
            .is_err()
        );
        assert!(confirmed(&"bad response".to_variant(), true).is_err());
    }
}
