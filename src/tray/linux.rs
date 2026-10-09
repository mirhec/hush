//! StatusNotifier with a real Activate handler and a host-rendered D-Bus menu.
//! Icon pixels travel over D-Bus, so Flatpak never exposes private icon files.
use super::{Event, Events, State};
use anyhow::{Result, anyhow};
use gio::{
    glib::{self, ToVariant, Variant},
    prelude::*,
};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, TryRecvError},
    },
    time::{Duration, Instant},
};

const ITEM: &str = "/StatusNotifierItem";
const MENU: &str = "/MenuBar";
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
const MENU_INTERFACE: &str = "com.canonical.dbusmenu";
type Properties = BTreeMap<String, Variant>;

struct Model {
    state: State,
    revision: u32,
}
impl Default for Model {
    fn default() -> Self {
        Self {
            state: State {
                language: crate::i18n::Language::System.resolved(),
                running: false,
                paused: false,
                unread: 0,
                warning: false,
            },
            revision: 1,
        }
    }
}
impl Model {
    fn title(&self) -> String {
        self.state.title()
    }
    fn properties(&self, id: i32, names: &[String]) -> Option<Properties> {
        let mut props = Properties::new();
        let mut set = |key: &str, value: Variant| {
            if names.is_empty() || names.iter().any(|name| name == key) {
                props.insert(key.into(), value);
            }
        };
        match id {
            0 => set("children-display", "submenu".to_variant()),
            2 | 8 => set("type", "separator".to_variant()),
            1 | 3..=7 | 9 => {
                let title = self.title();
                let label = match id {
                    1 => title.as_str(),
                    3 => "Hush öffnen",
                    4 => "Einstellungen",
                    5 => "Jetzt aktualisieren",
                    6 if self.state.paused => "Benachrichtigungen fortsetzen",
                    6 => "30 Minuten pausieren",
                    7 => "Dienst starten",
                    9 => "Hush beenden",
                    _ => unreachable!(),
                };
                set("label", self.state.language.text(label).to_variant());
                set("enabled", self.enabled(id).to_variant());
            }
            _ => return None,
        }
        set("visible", true.to_variant());
        Some(props)
    }
    fn enabled(&self, id: i32) -> bool {
        match id {
            3 | 4 | 6 | 9 => true,
            5 => self.state.running,
            7 => !self.state.running,
            _ => false,
        }
    }
    fn layout(&self, id: i32, depth: i32, names: &[String]) -> Option<Variant> {
        let props = self.properties(id, names)?;
        let children: Vec<Variant> = if id == 0 && depth != 0 {
            (1..=9).filter_map(|id| self.layout(id, 0, names)).collect()
        } else {
            Vec::new()
        };
        Some((id, props, children).to_variant())
    }
    fn click(&self, id: i32, name: &str, events: &Events) {
        if name != "clicked" || !self.enabled(id) {
            return;
        }
        let event = match id {
            3 => Event::Open,
            4 => Event::Settings,
            5 => Event::Refresh,
            6 => Event::Pause,
            7 => Event::Start,
            9 => Event::Quit,
            _ => return,
        };
        events.send(event);
    }
    fn menu_call(&self, method: &str, args: &Variant, events: &Events) -> Option<Variant> {
        match method {
            "GetLayout" => {
                let (id, depth, names) = args.get::<(i32, i32, Vec<String>)>()?;
                Some(Variant::tuple_from_iter([
                    self.revision.to_variant(),
                    self.layout(id, depth, &names)?,
                ]))
            }
            "GetGroupProperties" => {
                let (mut ids, names) = args.get::<(Vec<i32>, Vec<String>)>()?;
                if ids.is_empty() {
                    ids = (0..=9).collect();
                }
                let props: Vec<_> = ids
                    .into_iter()
                    .filter_map(|id| self.properties(id, &names).map(|p| (id, p)))
                    .collect();
                Some((props,).to_variant())
            }
            "GetProperty" => {
                let (id, name) = args.get::<(i32, String)>()?;
                Some((self.properties(id, &[])?.get(&name)?.clone(),).to_variant())
            }
            "Event" => {
                let (id, name, _, _) = args.get::<(i32, String, Variant, u32)>()?;
                self.properties(id, &[])?;
                self.click(id, &name, events);
                Some(().to_variant())
            }
            "EventGroup" => {
                let (items,) = args.get::<(Vec<(i32, String, Variant, u32)>,)>()?;
                let mut errors = Vec::<i32>::new();
                for (id, name, _, _) in items {
                    if self.properties(id, &[]).is_none() {
                        errors.push(id);
                    } else {
                        self.click(id, &name, events);
                    }
                }
                Some((errors,).to_variant())
            }
            "AboutToShow" => Some((false,).to_variant()),
            "AboutToShowGroup" => Some((Vec::<i32>::new(), Vec::<i32>::new()).to_variant()),
            _ => None,
        }
    }
    fn item_property(&self, name: &str) -> Variant {
        match name {
            "Category" => "ApplicationStatus".to_variant(),
            "Id" => crate::model::APP_ID.to_variant(),
            "Title" => self.title().to_variant(),
            "Status" => "Active".to_variant(),
            "WindowId" => 0u32.to_variant(),
            "ItemIsMenu" => false.to_variant(),
            "Menu" => glib::variant::ObjectPath::try_from(MENU)
                .unwrap()
                .to_variant(),
            "IconPixmap" => pixmap().to_variant(),
            "AttentionIconPixmap" | "OverlayIconPixmap" => {
                Vec::<(i32, i32, Vec<u8>)>::new().to_variant()
            }
            "ToolTip" => ("", pixmap(), "Hush", self.title()).to_variant(),
            _ => "".to_variant(),
        }
    }
}

fn activate(method: &str, events: &Events) {
    if matches!(method, "Activate" | "SecondaryActivate") {
        events.send(Event::Open);
    }
}
fn pixmap() -> Vec<(i32, i32, Vec<u8>)> {
    let icon = super::icon();
    let pixels = icon
        .rgba
        .chunks_exact(4)
        .flat_map(|p| [p[3], p[0], p[1], p[2]])
        .collect();
    vec![(icon.width as i32, icon.height as i32, pixels)]
}

pub(super) fn run(events: Events, updates: Receiver<Option<State>>) -> Result<()> {
    let context = glib::MainContext::new();
    context.with_thread_default(|| run_inner(&context, events, updates))?
}
fn run_inner(
    context: &glib::MainContext,
    events: Events,
    updates: Receiver<Option<State>>,
) -> Result<()> {
    let address =
        gio::dbus_address_get_for_bus_sync(gio::BusType::Session, None::<&gio::Cancellable>)?;
    let bus = gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        None::<&gio::Cancellable>,
    )?;
    bus.set_exit_on_close(false);
    let info = gio::DBusNodeInfo::for_xml(include_str!("status-notifier.xml"))?;
    let model = Arc::new(Mutex::new(Model::default()));
    let item_model = model.clone();
    let activation = events.clone();
    let item_id = bus.register_object(
        ITEM,
        &info.lookup_interface(ITEM_INTERFACE).unwrap(),
        move |_, _, _, _, method, _, reply| {
            activate(method, &activation);
            reply.return_value(Some(&().to_variant()));
        },
        move |_, _, _, _, name| item_model.lock().unwrap().item_property(name),
        |_, _, _, _, _, _| false,
    )?;
    let menu_model = model.clone();
    let menu_events = events.clone();
    let menu_id = bus.register_object(
        MENU,
        &info.lookup_interface(MENU_INTERFACE).unwrap(),
        move |_, _, _, _, method, args, reply| match menu_model.lock().unwrap().menu_call(
            method,
            &args,
            &menu_events,
        ) {
            Some(value) => reply.return_value(Some(&value)),
            None => reply.return_dbus_error(
                "com.canonical.dbusmenu.Error.InvalidMenuItem",
                "Unknown item or invalid arguments",
            ),
        },
        |_, _, _, _, name| match name {
            "Version" => 3u32.to_variant(),
            "TextDirection" => "ltr".to_variant(),
            "IconThemePath" => Vec::<String>::new().to_variant(),
            _ => "normal".to_variant(),
        },
        |_, _, _, _, _, _| false,
    )?;
    let watcher = gio::DBusProxy::new_sync(
        &bus,
        gio::DBusProxyFlags::DO_NOT_AUTO_START,
        None,
        Some("org.kde.StatusNotifierWatcher"),
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
        None::<&gio::Cancellable>,
    )?;
    let main_loop = glib::MainLoop::new(Some(context), false);
    let loop_handle = main_loop.clone();
    let connection = bus.clone();
    let registered = Arc::new(AtomicBool::new(false));
    let pending = Arc::new(AtomicBool::new(false));
    let mut owner = None;
    let mut last_attempt = Instant::now() - Duration::from_secs(10);
    let mut available = None;
    let source = glib::timeout_source_new(
        Duration::from_millis(250),
        Some("hush-tray"),
        glib::Priority::DEFAULT,
        move || {
            if connection.is_closed() {
                events.send(Event::Error(
                    "Verbindung zum Session-Bus wurde beendet.".into(),
                ));
                loop_handle.quit();
                return glib::ControlFlow::Break;
            }
            let current_owner = watcher.name_owner();
            if owner != current_owner {
                owner = current_owner.clone();
                registered.store(false, Ordering::Relaxed);
                last_attempt = Instant::now() - Duration::from_secs(10);
            }
            if current_owner.is_some()
                && !registered.load(Ordering::Relaxed)
                && !pending.load(Ordering::Relaxed)
                && last_attempt.elapsed() >= Duration::from_secs(5)
            {
                pending.store(true, Ordering::Relaxed);
                last_attempt = Instant::now();
                let registered = registered.clone();
                let pending = pending.clone();
                let proxy = watcher.clone();
                let sink = events.clone();
                // Register by object path + unique bus sender: no PID-derived name,
                // no collisions between Flatpak sandboxes, no wildcard own-name grant.
                watcher.call(
                    "RegisterStatusNotifierItem",
                    Some(&(ITEM,).to_variant()),
                    gio::DBusCallFlags::NONE,
                    3000,
                    None::<&gio::Cancellable>,
                    move |result| {
                        pending.store(false, Ordering::Relaxed);
                        if proxy.name_owner() == current_owner {
                            registered.store(result.is_ok(), Ordering::Relaxed);
                            if let Err(error) = result {
                                sink.send(Event::Error(format!(
                                    "Tray-Registrierung fehlgeschlagen: {error}"
                                )));
                            }
                        }
                    },
                );
            }
            let connected = registered.load(Ordering::Relaxed)
                && watcher
                    .cached_property("IsStatusNotifierHostRegistered")
                    .and_then(|v| v.get::<bool>())
                    .unwrap_or(false);
            if available != Some(connected) && (connected || !pending.load(Ordering::Relaxed)) {
                available = Some(connected);
                events.send(Event::Available(connected));
            }
            loop {
                match updates.try_recv() {
                    Ok(Some(state)) => {
                        let mut model = model.lock().unwrap();
                        model.state = state;
                        model.revision = model.revision.wrapping_add(1);
                        let _ = connection.emit_signal(
                            None,
                            MENU,
                            MENU_INTERFACE,
                            "LayoutUpdated",
                            Some(&(model.revision, 0i32).to_variant()),
                        );
                        let _ =
                            connection.emit_signal(None, ITEM, ITEM_INTERFACE, "NewTitle", None);
                        let _ =
                            connection.emit_signal(None, ITEM, ITEM_INTERFACE, "NewToolTip", None);
                    }
                    Ok(None) | Err(TryRecvError::Disconnected) => {
                        loop_handle.quit();
                        return glib::ControlFlow::Break;
                    }
                    Err(TryRecvError::Empty) => break,
                }
            }
            glib::ControlFlow::Continue
        },
    );
    source.attach(Some(context));
    main_loop.run();
    source.destroy();
    bus.unregister_object(item_id).map_err(|e| anyhow!("{e}"))?;
    bus.unregister_object(menu_id).map_err(|e| anyhow!("{e}"))?;
    bus.close_sync(None::<&gio::Cancellable>)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    #[ignore = "Requires HUSH_ISOLATED_DBUS_TEST=1 and dbus-run-session"]
    fn session_bus_activation_menu_and_host_restart() {
        assert_eq!(std::env::var("HUSH_ISOLATED_DBUS_TEST").as_deref(), Ok("1"));
        let context = glib::MainContext::new();
        context
            .with_thread_default(|| {
                let bus =
                    gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).unwrap();
                let name = "org.kde.StatusNotifierWatcher";
                let call_bus = |method, parameters: &Variant| {
                    bus.call_sync(
                        Some("org.freedesktop.DBus"),
                        "/org/freedesktop/DBus",
                        "org.freedesktop.DBus",
                        method,
                        Some(parameters),
                        None,
                        gio::DBusCallFlags::NONE,
                        2000,
                        None::<&gio::Cancellable>,
                    )
                    .unwrap()
                };
                // Never replace a real desktop watcher, even if invoked incorrectly.
                assert_eq!(
                    call_bus("NameHasOwner", &(name,).to_variant()).get::<(bool,)>(),
                    Some((false,))
                );
                assert_eq!(
                    call_bus("RequestName", &(name, 4u32).to_variant()).get::<(u32,)>(),
                    Some((1,))
                );
                let info = gio::DBusNodeInfo::for_xml(
                    r#"<node><interface name="org.kde.StatusNotifierWatcher">
                <method name="RegisterStatusNotifierItem"><arg type="s" direction="in"/></method>
                <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
                <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
                <property name="ProtocolVersion" type="i" access="read"/>
            </interface></node>"#,
                )
                .unwrap();
                let (registration_tx, registrations) = mpsc::channel();
                let id = bus
                    .register_object(
                        "/StatusNotifierWatcher",
                        &info.lookup_interface(name).unwrap(),
                        move |_, sender, _, _, _, parameters, reply| {
                            let (path,) = parameters.get::<(String,)>().unwrap();
                            registration_tx.send((sender.to_owned(), path)).unwrap();
                            reply.return_value(Some(&().to_variant()));
                        },
                        |_, _, _, _, name| match name {
                            "IsStatusNotifierHostRegistered" => true.to_variant(),
                            "ProtocolVersion" => 0i32.to_variant(),
                            _ => Vec::<String>::new().to_variant(),
                        },
                        |_, _, _, _, _, _| false,
                    )
                    .unwrap();
                let (event_tx, events) = mpsc::channel();
                let (updates, commands) = mpsc::channel();
                let worker = std::thread::spawn(move || {
                    run(
                        Events {
                            tx: event_tx,
                            ctx: None,
                        },
                        commands,
                    )
                });
                let wait = |ready: &mut dyn FnMut() -> bool| {
                    let until = Instant::now() + Duration::from_secs(8);
                    loop {
                        while context.pending() {
                            context.iteration(false);
                        }
                        if ready() {
                            break;
                        }
                        assert!(
                            Instant::now() < until,
                            "Timed out waiting for tray D-Bus event"
                        );
                        std::thread::sleep(Duration::from_millis(10));
                    }
                };
                let mut registered = None;
                wait(&mut || {
                    registered = registrations.try_recv().ok();
                    registered.is_some()
                });
                let (peer, path) = registered.unwrap();
                assert_eq!(path, ITEM);
                wait(&mut || {
                    events
                        .try_iter()
                        .any(|event| matches!(event, Event::Available(true)))
                });
                let call_item = |method, parameters: &Variant| {
                    bus.call_sync(
                        Some(&peer),
                        ITEM,
                        ITEM_INTERFACE,
                        method,
                        Some(parameters),
                        None,
                        gio::DBusCallFlags::NONE,
                        2000,
                        None::<&gio::Cancellable>,
                    )
                    .unwrap()
                };
                call_item("Activate", &(0i32, 0i32).to_variant());
                wait(&mut || events.try_iter().any(|event| matches!(event, Event::Open)));
                let layout = bus
                    .call_sync(
                        Some(&peer),
                        MENU,
                        MENU_INTERFACE,
                        "GetLayout",
                        Some(&(0i32, -1i32, Vec::<String>::new()).to_variant()),
                        None,
                        gio::DBusCallFlags::NONE,
                        2000,
                        None::<&gio::Cancellable>,
                    )
                    .unwrap();
                assert_eq!(layout.type_().as_str(), "(u(ia{sv}av))");
                bus.call_sync(
                    Some(&peer),
                    MENU,
                    MENU_INTERFACE,
                    "Event",
                    Some(&(4i32, "clicked", 0i32.to_variant(), 0u32).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    2000,
                    None::<&gio::Cancellable>,
                )
                .unwrap();
                wait(&mut || {
                    events
                        .try_iter()
                        .any(|event| matches!(event, Event::Settings))
                });
                call_bus("ReleaseName", &(name,).to_variant());
                wait(&mut || {
                    events
                        .try_iter()
                        .any(|event| matches!(event, Event::Available(false)))
                });
                call_bus("RequestName", &(name, 4u32).to_variant());
                wait(&mut || registrations.try_recv().is_ok());
                wait(&mut || {
                    events
                        .try_iter()
                        .any(|event| matches!(event, Event::Available(true)))
                });
                updates.send(None).unwrap();
                wait(&mut || worker.is_finished());
                worker.join().unwrap().unwrap();
                assert_eq!(
                    call_bus("NameHasOwner", &(peer,).to_variant()).get::<(bool,)>(),
                    Some((false,))
                );
                bus.unregister_object(id).unwrap();
                call_bus("ReleaseName", &(name,).to_variant());
            })
            .unwrap();
    }

    #[test]
    fn changing_language_updates_existing_tray_labels_and_status() {
        use crate::i18n::Language;
        let mut model = Model::default();
        model.state.language = Language::De;
        model.state.running = true;
        model.state.unread = 12;
        let label = |model: &Model, id| {
            model.properties(id, &[]).unwrap()["label"]
                .get::<String>()
                .unwrap()
        };
        assert_eq!(label(&model, 4), "Einstellungen");
        assert_eq!(model.title(), "Hush · Aktiv · 12 ungelesen");
        let old_state = model.state.clone();
        model.state.language = Language::En;
        assert!(old_state != model.state);
        assert_eq!(label(&model, 4), "Settings");
        assert_eq!(label(&model, 6), "Pause for 30 minutes");
        assert_eq!(model.title(), "Hush · Active · 12 unread");
        model.state.paused = true;
        assert_eq!(label(&model, 6), "Resume notifications");
        assert_eq!(model.title(), "Hush · Paused · 12 unread");
        model.state.language = Language::Ja;
        assert_eq!(label(&model, 3), Language::Ja.text("Hush öffnen"));
        assert_eq!(label(&model, 9), Language::Ja.text("Hush beenden"));
    }

    #[test]
    fn primary_activation_opens_ui_but_context_menu_does_not() {
        let (tx, rx) = mpsc::channel();
        let events = Events { tx, ctx: None };
        assert_eq!(
            Model::default().item_property("ItemIsMenu").get::<bool>(),
            Some(false)
        );
        activate("Activate", &events);
        assert!(matches!(rx.try_recv(), Ok(Event::Open)));
        activate("ContextMenu", &events);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn exported_menu_has_correct_wire_types_and_routes_enabled_actions() {
        let info = gio::DBusNodeInfo::for_xml(include_str!("status-notifier.xml")).unwrap();
        assert!(info.lookup_interface(ITEM_INTERFACE).is_some());
        assert!(info.lookup_interface(MENU_INTERFACE).is_some());
        let (tx, rx) = mpsc::channel();
        let events = Events { tx, ctx: None };
        let mut model = Model::default();
        let layout = model
            .menu_call(
                "GetLayout",
                &(0i32, -1i32, Vec::<String>::new()).to_variant(),
                &events,
            )
            .unwrap();
        assert_eq!(layout.type_().as_str(), "(u(ia{sv}av))");
        assert_eq!(layout.child_value(1).child_value(2).n_children(), 9);
        let get = model
            .menu_call("GetProperty", &(3i32, "label").to_variant(), &events)
            .unwrap();
        assert_eq!(get.type_().as_str(), "(v)");
        assert_eq!(
            model.item_property("IconPixmap").type_().as_str(),
            "a(iiay)"
        );
        assert_eq!(
            model.item_property("ToolTip").type_().as_str(),
            "(sa(iiay)ss)"
        );
        assert_eq!(model.item_property("Menu").type_().as_str(), "o");
        let click = |id| (id, "clicked", 0i32.to_variant(), 0u32).to_variant();
        model.menu_call("Event", &click(3i32), &events).unwrap();
        assert!(matches!(rx.try_recv(), Ok(Event::Open)));
        model.menu_call("Event", &click(5i32), &events).unwrap();
        assert!(rx.try_recv().is_err());
        model.state.running = true;
        model.menu_call("Event", &click(5i32), &events).unwrap();
        assert!(matches!(rx.try_recv(), Ok(Event::Refresh)));
        assert!(
            model
                .menu_call(
                    "GetLayout",
                    &(99i32, -1i32, Vec::<String>::new()).to_variant(),
                    &events
                )
                .is_none()
        );
    }
}
