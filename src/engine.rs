//! One background process per OS account, communicating with the GUI through private SQLite.
//! No localhost HTTP server; no credential IPC; no writable commands downloaded from GitHub.
use crate::{
    api::{Api, ApiError, ApiResult, validate_scopes},
    filter,
    model::{Config, Event, Repo, RepositoryCheck, RuntimeStatus, ThreadTask},
    notify,
    secrets::{self, Slot},
    storage::{Paths, Store},
};
use anyhow::{Context, Result, ensure};
use chrono::Utc;
use fs2::FileExt;
use std::{
    collections::HashSet,
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub fn is_running(paths: &Paths) -> Result<bool> {
    let file = paths.lock_file()?;
    match file.try_lock_exclusive() {
        Ok(()) => {
            FileExt::unlock(&file)?;
            Ok(false)
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(true),
        Err(e) => Err(e.into()),
    }
}
pub fn launch(paths: &Paths) -> Result<()> {
    // Serialize launchers so they cannot truncate each other's error log or
    // mistake the previous process's heartbeat for a successful fresh start.
    let launch_lock = paths.launch_lock_file()?;
    let waiting = Instant::now();
    loop {
        match launch_lock.try_lock_exclusive() {
            Ok(()) => break,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                ensure!(
                    waiting.elapsed() < Duration::from_secs(12),
                    "Ein anderer Dienststart ist noch nicht abgeschlossen."
                );
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e.into()),
        }
    }
    let store = Store::open(paths)?;
    let waiting = Instant::now();
    while is_running(paths)? {
        if store.status()?.alive() {
            return Ok(());
        }
        ensure!(
            waiting.elapsed() < Duration::from_secs(10),
            "Der Hintergrunddienst läuft, antwortet aber nicht. Bitte den Dienst beenden und erneut starten."
        );
        thread::sleep(Duration::from_millis(50));
    }
    store.put("heartbeat", "0")?;
    let log = paths
        .service_log()
        .context("Dienstprotokoll konnte nicht geöffnet werden.")?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--background")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log));
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .context("Hintergrundprozess konnte nicht gestartet werden.")?;
    let started = Instant::now();
    loop {
        if let Some(exit) = child.try_wait()? {
            // A competing launcher may have won the worker lock.
            if exit.success() && is_running(paths)? && store.status()?.alive() {
                return Ok(());
            }
            let details = std::fs::read_to_string(paths.log_path()).unwrap_or_default();
            anyhow::bail!(
                "Hintergrunddienst wurde beendet ({exit}). {} Protokoll: {}",
                details.trim(),
                paths.log_path().display()
            );
        }
        if is_running(paths)? && store.status()?.alive() {
            thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
        if started.elapsed() > Duration::from_secs(10) {
            thread::spawn(move || {
                let _ = child.wait();
            });
            anyhow::bail!(
                "Der Hintergrunddienst hat seinen Start nicht bestätigt. Protokoll: {}",
                paths.log_path().display()
            );
        }
        thread::sleep(Duration::from_millis(50));
    }
}

/// Called only on a GUI-owned helper thread. Never Debug-print either argument.
pub fn connect(
    paths: &Paths,
    token: Zeroizing<String>,
    details: Option<Zeroizing<String>>,
) -> Result<Config> {
    let mut store = Store::open(paths)?;
    let mut config = store.config()?;
    let replace_primary = !token.trim().is_empty();
    let token = connection_token(&config, token, || secrets::read(Slot::Notifications))?;
    let old_detail = if details.is_none() && config.has_detail_token {
        secrets::read(Slot::Details)?
    } else {
        None
    };
    let detail_ref = details.as_ref().or(old_detail.as_ref()).map(|s| s.as_str());
    let api = Api::new(token.as_str(), detail_ref)?;
    let (login, scopes) = api.identity(true)?;
    let read_org = validate_scopes(&scopes)?;
    ensure!(
        config.login.is_empty() || config.login.eq_ignore_ascii_case(&login),
        "Für einen Kontowechsel zuerst das bisherige Konto trennen. So bleiben Kontodaten getrennt."
    );
    if detail_ref.is_some() {
        let (detail_login, detail_scopes) = api.identity(false)?;
        ensure!(
            detail_login.eq_ignore_ascii_case(&login),
            "Beide Tokens müssen zum selben GitHub-Konto gehören."
        );
        ensure!(
            detail_scopes.is_empty(),
            "Für Details bitte einen Fine-grained Personal Access Token mit ausschließlich Leserechten verwenden."
        );
    }
    // Save only after both identities and the least-privilege primary scope have been checked.
    // OS keyring + SQLite cannot form a distributed transaction. If a later write fails,
    // a credential can remain in the keyring; the error is surfaced and no plaintext copy is made.
    if let Some(ref details) = details {
        secrets::save(Slot::Details, details.as_str())?;
    }
    if replace_primary {
        secrets::save(Slot::Notifications, token.as_str())?;
    }
    config.login = login;
    config.read_org = read_org;
    config.has_detail_token = detail_ref.is_some();
    let saved = store.save_config(&config)?;
    store.retry_pending_tasks()?;
    Ok(saved)
}

fn connection_token(
    config: &Config,
    submitted: Zeroizing<String>,
    read_existing: impl FnOnce() -> Result<Option<Zeroizing<String>>>,
) -> Result<Zeroizing<String>> {
    if !submitted.trim().is_empty() {
        return Ok(submitted);
    }
    ensure!(
        !config.login.is_empty(),
        "Zum ersten Verbinden einen Benachrichtigungs-Token eingeben."
    );
    read_existing()?.context("Gespeicherter Benachrichtigungs-Token fehlt. Bitte erneut eingeben.")
}

pub fn disconnect(paths: &Paths) -> Result<()> {
    let mut store = Store::open(paths)?;
    // Invalidate the account first. In-flight responses cannot be inserted into the new account.
    store.disconnect()?;
    let a = secrets::delete(Slot::Notifications);
    let b = secrets::delete(Slot::Details);
    a?;
    b?;
    Ok(())
}

struct Heartbeat {
    stop: mpsc::Sender<()>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Heartbeat {
    fn start(paths: &Paths) -> Result<Self> {
        // Open and publish synchronously: a failed heartbeat is a startup error.
        let store = Store::open(paths)?;
        store.put("heartbeat", &Utc::now().timestamp().to_string())?;
        let (stop, signal) = mpsc::channel();
        let thread = thread::spawn(move || {
            while signal.recv_timeout(Duration::from_secs(5))
                == Err(mpsc::RecvTimeoutError::Timeout)
            {
                if let Err(error) = store.put("heartbeat", &Utc::now().timestamp().to_string()) {
                    eprintln!("Heartbeat: {error:#}");
                }
            }
            let _ = store.put("heartbeat", "0");
        });
        Ok(Self {
            stop,
            thread: Some(thread),
        })
    }
}
impl Drop for Heartbeat {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn run_background(paths: Paths) -> Result<()> {
    let lock = paths.lock_file()?;
    match lock.try_lock_exclusive() {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    let mut store = Store::open(&paths)?;
    let result = (|| -> Result<()> {
        // Only the new lock owner clears stale stop requests. Direct
        // --background/autostart must restart after a previous --stop too.
        store.take_flag("shutdown")?;
        store.config()?;
        let mut status = store.status()?;
        status.phase = "Startet".into();
        status.service_error = None;
        store.save_status(&status)?;
        let _heartbeat = Heartbeat::start(&paths)?;
        run_loop(&mut store)
    })();
    let mut status = store.status().unwrap_or_default();
    status.phase = if result.is_ok() {
        "Beendet".into()
    } else {
        "Hintergrunddienst beendet · bitte neu starten".into()
    };
    status.service_error = result.as_ref().err().map(|error| format!("{error:#}"));
    status.heartbeat = 0;
    let _ = store.save_status(&status);
    let _ = store.put("heartbeat", "0");
    result
}

fn make_session(cfg: &Config) -> Result<Api> {
    let primary = secrets::read(Slot::Notifications)?
        .context("Kein Token im Schlüsselbund. Bitte neu verbinden.")?;
    let detail = if cfg.has_detail_token {
        secrets::read(Slot::Details)?
    } else {
        None
    };
    let api = Api::new(primary.as_str(), detail.as_ref().map(|s| s.as_str()))?;
    let (login, scopes) = api.identity(true)?;
    ensure!(
        login.eq_ignore_ascii_case(&cfg.login),
        "Gespeicherter Token gehört zu einem anderen Konto. Bitte neu verbinden."
    );
    validate_scopes(&scopes)?;
    Ok(api)
}
fn run_loop(store: &mut Store) -> Result<()> {
    let mut revision = u64::MAX;
    let mut api: Option<Api> = None;
    let mut next = Instant::now();
    let mut retry_floor = Instant::now();
    let mut failures = 0u32;
    let mut status = store.status()?;
    let mut teams = HashSet::new();
    let mut team_refreshed = Instant::now() - Duration::from_secs(3600);
    loop {
        if store.take_flag("shutdown")? {
            return Ok(());
        }
        if store.take_flag("test_notification")? {
            status.notification_error = notify::test().err().map(|e| e.to_string());
            store.save_status(&status)?;
        }
        let cfg = store.config()?;
        if cfg.revision != revision || cfg.login.is_empty() {
            revision = cfg.revision;
            api = None;
            teams.clear();
            team_refreshed = Instant::now() - Duration::from_secs(3600);
            failures = 0;
            next = Instant::now();
            // User edits cannot bypass GitHub's Retry-After / rate-limit wait.
        }
        let refresh = store.take_flag("refresh")?;
        if refresh {
            next = Instant::now();
        }
        if cfg.login.is_empty() {
            status.phase = "Nicht verbunden".into();
            status.next_sync = None;
            status.warnings.clear();
            store.save_status(&status)?;
            thread::sleep(Duration::from_secs(1));
            continue;
        }
        if Instant::now() < next || Instant::now() < retry_floor {
            thread::sleep(Duration::from_secs(1));
            continue;
        }
        if api.is_none() {
            match make_session(&cfg) {
                Ok(session) => {
                    api = Some(session);
                }
                Err(error) => {
                    status.phase = "Verbindung prüfen".into();
                    status.warnings = vec![error.to_string()];
                    let wait = if let Some(ApiError::RateLimit(until)) =
                        error.downcast_ref::<ApiError>()
                    {
                        (until - Utc::now().timestamp()).max(120) as u64
                    } else {
                        120
                    };
                    next = Instant::now() + Duration::from_secs(wait);
                    retry_floor = next;
                    status.next_sync = Some(Utc::now().timestamp() + wait as i64);
                    store.save_status(&status)?;
                    continue;
                }
            }
        }
        let Some(session) = api.as_ref() else {
            continue;
        };
        session.reset_cycle();
        status.phase = "Synchronisiert".into();
        status.warnings.clear();
        store.save_status(&status)?;
        let result = cycle(
            store,
            session,
            &cfg,
            &mut teams,
            &mut team_refreshed,
            &mut status,
        );
        let poll_floor = session.rate.borrow().poll_floor;
        let mut delay = cfg.interval_secs.max(poll_floor);
        match result {
            Ok(()) => {
                failures = 0;
                status.last_sync = Some(Utc::now().timestamp());
                status.phase = if status.warnings.is_empty() {
                    "Bereit".into()
                } else {
                    "Teilsynchronisiert".into()
                };
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                delay = delay.max((30u64.saturating_mul(1u64 << failures.min(6))).min(1800));
                if let Some(ApiError::RateLimit(until)) = error.downcast_ref::<ApiError>() {
                    delay = delay.max((until - Utc::now().timestamp()).max(60) as u64);
                }
                if matches!(
                    error.downcast_ref::<ApiError>(),
                    Some(ApiError::Unauthorized)
                ) {
                    api = None;
                }
                status.phase = "Wartet auf Verbindung".into();
                status.warnings.push(error.to_string());
                retry_floor = Instant::now() + Duration::from_secs(delay);
            }
        }
        if let Some(ref session) = api {
            status.requests_last_cycle = session.calls.get();
            status.remaining = session.rate.borrow().remaining;
            status.rate_reset = session.rate.borrow().reset;
        }
        // Fetch current settings again so a pause/disconnect during a long request takes effect.
        let current = store.config()?;
        if current.login == cfg.login {
            deliver_pending(store, &current, &mut status, notify::deliver)?;
        }
        status.next_sync = Some(Utc::now().timestamp() + delay as i64);
        status.warnings.sort();
        status.warnings.dedup();
        status.warnings.truncate(12);
        store.save_status(&status)?;
        next = Instant::now() + Duration::from_secs(delay);
        // A manual refresh or settings edit must also respect GitHub's minimum poll interval.
        retry_floor = retry_floor.max(Instant::now() + Duration::from_secs(poll_floor));
    }
}

fn deliver_pending(
    store: &Store,
    config: &Config,
    status: &mut RuntimeStatus,
    send: impl FnOnce(&[Event], &Config) -> Result<()>,
) -> Result<()> {
    let pending = store.outbox()?;
    let ids: Vec<_> = pending.iter().map(|event| event.id.clone()).collect();
    if config.paused() || !config.desktop_notifications {
        return store.notified(&ids);
    }
    let enabled: Vec<_> = pending
        .into_iter()
        .filter(|event| config.rules.allows(event.kind))
        .collect();
    if enabled.is_empty() {
        // An empty outbox is not evidence that a previous OS delivery error is fixed.
        return store.notified(&ids);
    }
    match send(&enabled, config) {
        Ok(()) => {
            store.notified(&ids)?;
            status.notification_error = None;
        }
        Err(error) => status.notification_error = Some(error.to_string()),
    }
    Ok(())
}

fn must_back_off(error: &ApiError) -> bool {
    matches!(
        error,
        ApiError::RateLimit(_) | ApiError::Budget | ApiError::Unauthorized | ApiError::Network
    )
}
fn cycle(
    store: &mut Store,
    api: &Api,
    cfg: &Config,
    teams: &mut HashSet<String>,
    team_refreshed: &mut Instant,
    status: &mut RuntimeStatus,
) -> Result<()> {
    let started = Utc::now();
    if cfg.read_org && team_refreshed.elapsed() > Duration::from_secs(900) {
        match api.teams() {
            Ok(found) => {
                *teams = found;
                *team_refreshed = Instant::now();
            }
            Err(e) if must_back_off(&e) => return Err(e.into()),
            Err(e) => status.warnings.push(format!("Automatische Teams: {e}")),
        }
    }
    teams.extend(cfg.manual_teams.iter().map(ToString::to_string));
    status.team_count = teams.len();
    if cfg.rules.requests && !cfg.read_org && cfg.manual_teams.is_empty() {
        status.warnings.push("Team-Reviews: Für automatische Mitgliedschaft read:org erlauben oder Teams in den Einstellungen eintragen.".into());
    }
    if cfg.rules.requests || cfg.rules.mentions || cfg.rules.reviews {
        let (since, quiet) = store.cursor("notifications")?;
        let tasks = api.notification_tasks(since, quiet)?;
        let mut unsupported = HashSet::new();
        for task in tasks {
            if ["Issue", "PullRequest", "Commit", "Discussion"]
                .contains(&task.subject_type.as_str())
                && !task.subject_url.is_empty()
            {
                store.enqueue(&cfg.login, &task)?;
            } else {
                unsupported.insert(task.subject_type.clone());
            }
        }
        if !unsupported.is_empty() {
            let mut kinds: Vec<_> = unsupported.into_iter().collect();
            kinds.sort();
            status.warnings.push(format!(
                "Nicht ausgewertete GitHub-Typen: {}. Diese werden nicht als Erwähnungen geraten.",
                kinds.join(", ")
            ));
        }
        store.advance(&cfg.login, "notifications", started)?;
    }
    if cfg.rules.reviews {
        let (since, quiet) = store.cursor("authored-prs")?;
        match api.authored_pr_tasks(&cfg.login, since, quiet) {
            Ok(tasks) => {
                for task in tasks {
                    store.enqueue(&cfg.login, &task)?;
                }
                store.advance(&cfg.login, "authored-prs", started)?;
            }
            Err(e) if must_back_off(&e) => return Err(e.into()),
            Err(e) => status.warnings.push(format!("Eigene Pull Requests: {e}")),
        }
    }
    sync_issues(store, cfg, status, started, |repo, since| {
        api.new_issues(repo, since)
    })?;
    for task in store.tasks(24)? {
        if !store.current_user(&cfg.login)? {
            return Ok(());
        }
        match process_thread(api, cfg, teams, &task) {
            Ok(events) => {
                store.ingest(&cfg.login, &events, task.quiet)?;
                store.task_done(&task.id)?;
            }
            Err(e) => {
                store.retry_task(&task.id)?;
                if must_back_off(&e) {
                    return Err(e.into());
                }
                status
                    .warnings
                    .push(format!("{} · {}: {e}", task.repository, task.subject_type));
            }
        }
    }
    let pending = store.pending_tasks()?;
    if pending > 0 {
        status.warnings.push(format!(
            "{pending} Threads warten auf Auswertung oder einen erneuten Versuch."
        ));
    }
    Ok(())
}

fn sync_issues(
    store: &mut Store,
    cfg: &Config,
    status: &mut RuntimeStatus,
    started: chrono::DateTime<Utc>,
    mut fetch: impl FnMut(&Repo, chrono::DateTime<Utc>) -> ApiResult<Vec<serde_json::Value>>,
) -> Result<()> {
    status.repositories.clear();
    if !cfg.rules.issues {
        return Ok(());
    }
    for repo in &cfg.repositories {
        if !store.current_user(&cfg.login)? {
            return Ok(());
        }
        let source = format!("issues:{repo}");
        let (since, quiet) = store.cursor(&source)?;
        let mut check = RepositoryCheck {
            repository: repo.to_string(),
            checked_at: Utc::now().timestamp(),
            error: None,
            imported: 0,
            initial_import: quiet,
        };
        match fetch(repo, since) {
            Ok(items) => {
                let events: Vec<_> = items
                    .iter()
                    .filter_map(|v| filter::new_issue(v, repo, since))
                    .collect();
                check.imported = store.ingest(&cfg.login, &events, quiet)?;
                store.advance(&cfg.login, &source, started)?;
            }
            Err(error) => {
                check.error = Some(error.to_string());
                status.repositories.push(check);
                if must_back_off(&error) {
                    return Err(error.into());
                }
                status.warnings.push(format!("{repo}: {error}"));
                continue;
            }
        }
        status.repositories.push(check);
    }
    Ok(())
}

fn process_thread(
    api: &Api,
    cfg: &Config,
    teams: &HashSet<String>,
    task: &ThreadTask,
) -> ApiResult<Vec<Event>> {
    let (kind, id) = filter::subject_locator(&task.subject_url, &task.repository)
        .map_err(|_| ApiError::Origin)?;
    let repo = &task.repository;
    let base = repo.api_path();
    let mut events = Vec::new();
    if kind == "issues" || kind == "pulls" {
        let issue = api.get(&format!("{base}/issues/{id}"))?;
        let title = issue["title"].as_str().unwrap_or(&task.title);
        let author = filter::str_at(&issue, &["user", "login"]);
        let is_pr = kind == "pulls" || issue.get("pull_request").is_some();
        if is_pr && cfg.rules.requests {
            for item in api.list(&format!("{base}/issues/{id}/timeline"), &[], false)? {
                if let Some(event) = filter::request(
                    &item,
                    repo,
                    title,
                    filter::str_at(&issue, &["html_url"]),
                    &cfg.login,
                    teams,
                    task.since,
                ) {
                    events.push(event);
                }
            }
        }
        if cfg.rules.mentions {
            let query = [("since", task.since.to_rfc3339())];
            for comment in api.list(&format!("{base}/issues/{id}/comments"), &query, false)? {
                if let Some(event) = filter::comment_mention(
                    &comment,
                    "issue-comment",
                    repo,
                    title,
                    &cfg.login,
                    task.since,
                ) {
                    events.push(event);
                }
            }
            if is_pr {
                for comment in api.list(&format!("{base}/pulls/{id}/comments"), &query, false)? {
                    if let Some(event) = filter::comment_mention(
                        &comment,
                        "review-comment",
                        repo,
                        title,
                        &cfg.login,
                        task.since,
                    ) {
                        events.push(event);
                    }
                }
            }
        }
        if is_pr && (cfg.rules.reviews || cfg.rules.mentions) {
            for item in api.list(&format!("{base}/pulls/{id}/reviews"), &[], false)? {
                if let Some(event) =
                    filter::review(&item, repo, title, author, &cfg.login, task.since)
                        .filter(|_| cfg.rules.reviews)
                {
                    events.push(event);
                } else if cfg.rules.mentions {
                    if let Some(event) = filter::comment_mention(
                        &item, "review", repo, title, &cfg.login, task.since,
                    ) {
                        events.push(event);
                    }
                }
            }
        }
    } else if kind == "commits" && cfg.rules.mentions {
        for comment in api.list(&format!("{base}/commits/{id}/comments"), &[], false)? {
            if let Some(event) = filter::comment_mention(
                &comment,
                "commit-comment",
                repo,
                &task.title,
                &cfg.login,
                task.since,
            ) {
                events.push(event);
            }
        }
    } else if kind == "discussions" && cfg.rules.mentions {
        let web_url = format!("https://github.com/{repo}/discussions/{id}");
        for comment in api.discussion_comments(&web_url)? {
            if let Some(event) = filter::comment_mention(
                &comment,
                "discussion-comment",
                repo,
                &task.title,
                &cfg.login,
                task.since,
            ) {
                events.push(event);
            }
        }
    }
    Ok(events)
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
