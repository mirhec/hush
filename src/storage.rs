use crate::model::{
    Config, Event, MAX_EVENTS, RETENTION_DAYS, RuntimeStatus, ThreadTask, parse_time,
};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, Utc};
use directories::ProjectDirs;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::Duration as StdDuration,
};

#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
}
impl Paths {
    pub fn discover() -> Result<Self> {
        let dirs = ProjectDirs::from("io", "Hush", "Hush")
            .context("Kein privates Benutzer-Datenverzeichnis verfügbar.")?;
        Self::at(dirs.data_local_dir().to_owned())
    }
    pub fn at(root: PathBuf) -> Result<Self> {
        if root.exists() {
            ensure!(
                !fs::symlink_metadata(&root)?.file_type().is_symlink(),
                "Datenverzeichnis darf kein Symlink sein."
            );
        }
        fs::create_dir_all(&root)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self { root })
    }
    pub fn db(&self) -> PathBuf {
        self.root.join("hush.sqlite3")
    }
    pub fn lock_file(&self) -> Result<File> {
        private_file(&self.root.join("engine.lock"))
    }
    pub fn window_lock_file(&self) -> Result<File> {
        private_file(&self.root.join("window.lock"))
    }
    pub fn tray_lock_file(&self) -> Result<File> {
        private_file(&self.root.join("tray.lock"))
    }
    pub fn window_log(&self) -> Result<File> {
        let file = private_file(&self.root.join("window.log"))?;
        file.set_len(0)?;
        Ok(file)
    }
    pub fn credentials_lock_file(&self) -> Result<File> {
        private_file(&self.root.join("credentials.lock"))
    }
    pub fn launch_lock_file(&self) -> Result<File> {
        private_file(&self.root.join("launch.lock"))
    }
    pub fn log_path(&self) -> PathBuf {
        self.root.join("service.log")
    }
    pub fn service_log(&self) -> Result<File> {
        let file = private_file(&self.log_path())?;
        file.set_len(0)?;
        Ok(file)
    }
}

fn private_file(path: &Path) -> Result<File> {
    if let Ok(meta) = fs::symlink_metadata(path) {
        ensure!(
            meta.is_file() && !meta.file_type().is_symlink(),
            "Datendatei muss eine reguläre Datei sein."
        );
    }
    let mut opts = OpenOptions::new();
    opts.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let file = opts.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    Ok(file)
}

pub struct Store {
    conn: Connection,
}
impl Store {
    pub fn open(paths: &Paths) -> Result<Self> {
        // Never open-and-close an existing SQLite database outside SQLite.
        // POSIX close releases *all* this process's locks for that inode,
        // including locks held by the worker's other SQLite connection. That
        // lets another process delete a still-active WAL and split IPC state.
        let db = paths.db();
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&db) {
            Ok(file) => drop(file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
        let metadata = fs::symlink_metadata(&db)?;
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Datenbank muss eine reguläre Datei sein."
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&db, fs::Permissions::from_mode(0o600))?;
        }
        let conn = Connection::open(paths.db())?;
        conn.busy_timeout(StdDuration::from_secs(5))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;
          CREATE TABLE IF NOT EXISTS kv(key TEXT PRIMARY KEY,value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS events(id TEXT PRIMARY KEY,payload TEXT NOT NULL,at INTEGER NOT NULL,unread INTEGER NOT NULL DEFAULT 1,archived INTEGER NOT NULL DEFAULT 0,notified INTEGER NOT NULL DEFAULT 0);
          CREATE TABLE IF NOT EXISTS seen(id TEXT PRIMARY KEY,at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS tasks(id TEXT PRIMARY KEY,payload TEXT NOT NULL,due INTEGER NOT NULL,attempt INTEGER NOT NULL DEFAULT 0);
          CREATE INDEX IF NOT EXISTS event_date ON events(at DESC);
          CREATE INDEX IF NOT EXISTS task_due ON tasks(due);")?;
        Ok(Self { conn })
    }
    pub(crate) fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key=?1", [key], |r| r.get(0))
            .optional()?)
    }
    pub fn put(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT INTO kv(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key, value])?;
        Ok(())
    }
    pub fn config(&self) -> Result<Config> {
        match self.get("config")? {
            Some(v) => Ok(serde_json::from_str(&v)
                .context("Einstellungen sind beschädigt. Keine Daten wurden überschrieben.")?),
            None => Ok(Config::default()),
        }
    }
    // All read-modify-write transactions reserve the write lock before their
    // first SELECT. DEFERRED upgrades can fail immediately with SQLITE_BUSY
    // when the worker, heartbeat and GUI write concurrently, bypassing the timeout.
    pub fn save_config(&mut self, config: &Config) -> Result<Config> {
        self.update_config(|_| config.clone())
    }
    /// A settings draft may predate an account change in another thread/process.
    /// Preserve authentication and pause state under the same database write lock.
    pub fn save_preferences(&mut self, preferences: &Config) -> Result<Config> {
        self.update_config(|current| {
            let mut next = preferences.clone();
            next.login = current.login.clone();
            next.oauth = current.oauth;
            next.has_detail_token = current.has_detail_token;
            next.read_org = current.read_org;
            next.paused_until = current.paused_until;
            next
        })
    }
    /// Identity/keyring checks may take time. Commit only their account fields,
    /// preserving preferences and tray changes made while those checks ran.
    pub fn save_account(&mut self, account: &Config) -> Result<Config> {
        self.update_config(|current| {
            let mut next = current.clone();
            next.login = account.login.clone();
            next.oauth = account.oauth;
            next.has_detail_token = account.has_detail_token;
            next.read_org = account.read_org;
            next
        })
    }
    /// Appearance changes save immediately without committing unrelated settings drafts.
    pub fn save_appearance(
        &mut self,
        language: crate::i18n::Language,
        light_theme: bool,
    ) -> Result<Config> {
        self.update_config(|current| {
            let mut next = current.clone();
            next.language = language;
            next.light_theme = light_theme;
            next
        })
    }
    /// UI and tray toggles always use the latest stored state, never a snapshot.
    pub fn toggle_pause(&mut self) -> Result<Config> {
        self.update_config(|current| {
            let mut next = current.clone();
            next.paused_until = if current.paused() {
                0
            } else {
                Utc::now().timestamp() + 1800
            };
            next
        })
    }
    fn update_config(&mut self, update: impl FnOnce(&Config) -> Config) -> Result<Config> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let old: Option<String> = tx
            .query_row("SELECT value FROM kv WHERE key='config'", [], |r| r.get(0))
            .optional()?;
        let previous: Config = old
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or_default();
        let mut next = update(&previous);
        next.validate()?;
        next.revision = previous.revision.saturating_add(1);
        tx.execute("INSERT INTO kv(key,value) VALUES('config',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&next)?])?;
        tx.execute("INSERT INTO kv(key,value) VALUES('refresh','1') ON CONFLICT(key) DO UPDATE SET value='1'", [])?;
        tx.commit()?;
        Ok(next)
    }
    pub fn current_user(&self, login: &str) -> Result<bool> {
        Ok(!login.is_empty() && self.config()?.login.eq_ignore_ascii_case(login))
    }
    pub fn status(&self) -> Result<RuntimeStatus> {
        let mut status: RuntimeStatus = self
            .get("status")?
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or_default();
        status.heartbeat = self
            .get("heartbeat")?
            .and_then(|v| v.parse().ok())
            .unwrap_or_default();
        Ok(status)
    }
    pub fn save_status(&self, status: &RuntimeStatus) -> Result<()> {
        self.put("status", &serde_json::to_string(status)?)
    }
    pub fn request_refresh(&self) -> Result<()> {
        self.put("refresh", "1")
    }
    pub fn request_stop(&self) -> Result<()> {
        self.put("shutdown", "1")
    }
    pub fn take_flag(&mut self, key: &str) -> Result<bool> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value: Option<String> = tx
            .query_row("SELECT value FROM kv WHERE key=?1", [key], |r| r.get(0))
            .optional()?;
        tx.execute("DELETE FROM kv WHERE key=?1", [key])?;
        tx.commit()?;
        Ok(value.as_deref() == Some("1"))
    }
    /// First import is the last 24 hours, but its events never generate banners.
    pub fn cursor(&self, source: &str) -> Result<(DateTime<Utc>, bool)> {
        match self.get(&format!("cursor:{source}"))? {
            Some(v) => Ok((
                parse_time(&v).context("Ungültiger Sync-Zeitpunkt.")? - Duration::minutes(2),
                false,
            )),
            None => Ok((Utc::now() - Duration::hours(24), true)),
        }
    }
    pub fn advance(&mut self, login: &str, source: &str, to: DateTime<Utc>) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let raw: Option<String> = tx
            .query_row("SELECT value FROM kv WHERE key='config'", [], |r| r.get(0))
            .optional()?;
        let cfg: Config = raw
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or_default();
        if !login.is_empty() && cfg.login.eq_ignore_ascii_case(login) {
            tx.execute("INSERT INTO kv(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![format!("cursor:{source}"),to.to_rfc3339()])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn enqueue(&mut self, login: &str, task: &ThreadTask) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let raw: String =
            tx.query_row("SELECT value FROM kv WHERE key='config'", [], |r| r.get(0))?;
        let cfg: Config = serde_json::from_str(&raw)?;
        if !cfg.login.eq_ignore_ascii_case(login) {
            return Ok(());
        }
        let old: Option<String> = tx
            .query_row("SELECT payload FROM tasks WHERE id=?1", [&task.id], |r| {
                r.get(0)
            })
            .optional()?;
        let mut next = task.clone();
        if let Some(old) = old {
            let previous: ThreadTask = serde_json::from_str(&old)?;
            // Do not promote an unfinished quiet history import into a banner avalanche.
            // Prefer the newer boundary when quiet and live work collide.
            next.since = if next.quiet == previous.quiet {
                next.since.min(previous.since)
            } else {
                next.since.max(previous.since)
            };
            next.quiet = next.quiet && previous.quiet;
        }
        tx.execute("INSERT INTO tasks(id,payload,due) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload", params![next.id, serde_json::to_string(&next)?, Utc::now().timestamp()])?;
        tx.commit()?;
        Ok(())
    }
    pub fn tasks(&self, limit: usize) -> Result<Vec<ThreadTask>> {
        let mut statement = self
            .conn
            .prepare("SELECT payload FROM tasks WHERE due<=?1 ORDER BY due,id LIMIT ?2")?;
        let rows = statement.query_map(params![Utc::now().timestamp(), limit], |r| {
            r.get::<_, String>(0)
        })?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn task_done(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM tasks WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn retry_task(&self, id: &str) -> Result<()> {
        self.conn.execute("UPDATE tasks SET attempt=attempt+1,due=?2+MIN(3600,60*(1 << MIN(attempt,6))) WHERE id=?1", params![id,Utc::now().timestamp()])?;
        Ok(())
    }
    pub fn retry_pending_tasks(&self) -> Result<()> {
        self.conn.execute("UPDATE tasks SET due=0,attempt=0", [])?;
        Ok(())
    }
    pub fn pending_tasks(&self) -> Result<usize> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))?)
    }
    /// Transactional dedup and durable notification outbox. Checks account identity under the same write lock.
    pub fn ingest(&mut self, login: &str, events: &[Event], quiet: bool) -> Result<usize> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let raw: Option<String> = tx
            .query_row("SELECT value FROM kv WHERE key='config'", [], |r| r.get(0))
            .optional()?;
        let cfg: Config = raw
            .map(|v| serde_json::from_str(&v))
            .transpose()?
            .unwrap_or_default();
        if cfg.login.is_empty() || !cfg.login.eq_ignore_ascii_case(login) {
            return Ok(0);
        }
        let mut count = 0;
        for event in events {
            let fresh = tx.execute(
                "INSERT OR IGNORE INTO seen(id,at) VALUES(?1,?2)",
                params![event.id, Utc::now().timestamp()],
            )?;
            if fresh == 0 || !cfg.rules.allows(event.kind) {
                continue;
            }
            let notified = quiet || cfg.paused() || !cfg.desktop_notifications;
            tx.execute(
                "INSERT OR IGNORE INTO events(id,payload,at,notified) VALUES(?1,?2,?3,?4)",
                params![
                    event.id,
                    serde_json::to_string(event)?,
                    event.occurred_at.timestamp(),
                    notified
                ],
            )?;
            count += 1;
        }
        tx.commit()?;
        self.prune()?;
        Ok(count)
    }
    pub fn events(&self) -> Result<Vec<Event>> {
        let mut statement = self
            .conn
            .prepare("SELECT payload,unread,archived FROM events ORDER BY at DESC LIMIT ?1")?;
        let rows = statement.query_map([MAX_EVENTS], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, bool>(1)?,
                r.get::<_, bool>(2)?,
            ))
        })?;
        rows.map(|r| {
            let (s, unread, legacy_archived) = r?;
            let mut e: Event = serde_json::from_str(&s)?;
            // Older versions offered a local archive. Keep those events in the
            // shared history, preserving their read status without a migration.
            e.unread = unread && !legacy_archived;
            Ok(e)
        })
        .collect()
    }
    pub fn outbox(&self) -> Result<Vec<Event>> {
        let mut statement = self
            .conn
            .prepare("SELECT payload FROM events WHERE notified=0 ORDER BY at ASC LIMIT 50")?;
        let rows = statement.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn notified(&self, ids: &[String]) -> Result<()> {
        for id in ids {
            self.conn
                .execute("UPDATE events SET notified=1 WHERE id=?1", [id])?;
        }
        Ok(())
    }
    pub fn read(&self, id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE events SET unread=0 WHERE id=?1", [id])?;
        Ok(())
    }
    pub fn read_all(&self) -> Result<()> {
        self.conn.execute("UPDATE events SET unread=0", [])?;
        Ok(())
    }
    pub fn clear_history(&self) -> Result<()> {
        // Keep the seen IDs/cursors, otherwise clearing the UI would replay old notifications.
        self.conn.execute("DELETE FROM events", [])?;
        Ok(())
    }
    pub fn disconnect(&mut self) -> Result<()> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let previous: Option<String> = tx
            .query_row("SELECT value FROM kv WHERE key='config'", [], |row| {
                row.get(0)
            })
            .optional()?;
        let previous: Config = previous
            .map(|raw| serde_json::from_str(&raw))
            .transpose()?
            .unwrap_or_default();
        // Language and theme belong to the app; account-specific settings are cleared.
        let reset = Config {
            language: previous.language,
            light_theme: previous.light_theme,
            revision: previous.revision.saturating_add(1),
            ..Default::default()
        };
        tx.execute_batch("DELETE FROM events; DELETE FROM tasks; DELETE FROM seen; DELETE FROM kv WHERE key LIKE 'cursor:%'; DELETE FROM kv WHERE key='config';")?;
        tx.execute(
            "INSERT INTO kv(key,value) VALUES('config',?1)",
            [serde_json::to_string(&reset)?],
        )?;
        tx.execute("INSERT INTO kv(key,value) VALUES('refresh','1') ON CONFLICT(key) DO UPDATE SET value='1'", [])?;
        tx.commit()?;
        Ok(())
    }
    fn prune(&self) -> Result<()> {
        self.conn.execute("DELETE FROM events WHERE at<?1 OR id IN (SELECT id FROM events ORDER BY at DESC LIMIT -1 OFFSET ?2)", params![(Utc::now()-Duration::days(RETENTION_DAYS)).timestamp(),MAX_EVENTS])?;
        self.conn.execute(
            "DELETE FROM seen WHERE at<?1",
            [(Utc::now() - Duration::days(90)).timestamp()],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Kind;
    fn event() -> Event {
        Event {
            id: "review:42".into(),
            kind: Kind::Review,
            title: "Test".into(),
            repository: "a/b".into(),
            actor: "bob".into(),
            detail: "Approved".into(),
            url: "https://github.com/a/b/pull/1".into(),
            occurred_at: Utc::now(),
            unread: true,
        }
    }
    #[test]
    fn durable_dedup_and_quiet_start() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut db = Store::open(&paths)?;
        let cfg = Config {
            login: "alice".into(),
            ..Default::default()
        };
        db.save_config(&cfg)?;
        assert_eq!(db.ingest("alice", &[event()], true)?, 1);
        assert!(db.outbox()?.is_empty());
        db.clear_history()?;
        assert_eq!(db.ingest("alice", &[event()], false)?, 0);
        assert_eq!(db.ingest("someone-else", &[event()], false)?, 0);
        Ok(())
    }
    #[test]
    fn preferences_never_serialize_a_token() -> Result<()> {
        let serialized = serde_json::to_string(&Config::default())?;
        assert!(!serialized.contains("ghp_"));
        assert!(!serialized.contains("password"));
        Ok(())
    }

    #[test]
    fn appearance_changes_preserve_account_preferences_and_pause() -> Result<()> {
        use crate::i18n::Language;
        let legacy: Config = serde_json::from_str(r#"{"login":"alice"}"#)?;
        assert_eq!(legacy.language, Language::System);
        assert!(!legacy.light_theme);
        let temporary = tempfile::tempdir()?;
        let paths = Paths::at(temporary.path().join("data"))?;
        let mut store = Store::open(&paths)?;
        let original = store.save_config(&Config {
            login: "alice".into(),
            oauth: true,
            paused_until: Utc::now().timestamp() + 1800,
            interval_secs: 300,
            ..Default::default()
        })?;
        store.save_appearance(Language::Ja, true)?;
        let saved = Store::open(&paths)?.config()?;
        assert_eq!(saved.language, Language::Ja);
        assert!(saved.light_theme && saved.oauth);
        assert_eq!(saved.login, original.login);
        assert_eq!(saved.paused_until, original.paused_until);
        assert_eq!(saved.interval_secs, 300);
        let mut stale_account = original;
        stale_account.login = "alice".into();
        let saved = store.save_account(&stale_account)?;
        assert_eq!(saved.language, Language::Ja);
        assert!(saved.light_theme);
        store.disconnect()?;
        let disconnected = Store::open(&paths)?.config()?;
        assert!(disconnected.login.is_empty() && !disconnected.oauth);
        assert_eq!(disconnected.language, Language::Ja);
        assert!(disconnected.light_theme);
        assert_eq!(disconnected.paused_until, 0);
        Ok(())
    }

    #[test]
    fn stale_preferences_preserve_a_new_oauth_session_and_pause() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut ui = Store::open(&paths)?;
        let mut worker = Store::open(&paths)?;
        ui.save_config(&Config {
            login: "alice".into(),
            has_detail_token: true,
            ..Default::default()
        })?;
        let mut draft = ui.config()?;
        draft.interval_secs = 300;
        let session = worker.save_config(&Config {
            login: "alice".into(),
            oauth: true,
            read_org: true,
            paused_until: Utc::now().timestamp() + 1800,
            ..Default::default()
        })?;
        ui.take_flag("refresh")?;
        let saved = ui.save_preferences(&draft)?;
        assert_eq!(saved.login, session.login);
        assert_eq!(saved.oauth, session.oauth);
        assert_eq!(saved.has_detail_token, session.has_detail_token);
        assert_eq!(saved.read_org, session.read_org);
        assert_eq!(saved.paused_until, session.paused_until);
        assert_eq!(saved.interval_secs, 300);
        assert_eq!(saved.revision, session.revision + 1);
        assert!(ui.take_flag("refresh")?);
        Ok(())
    }

    #[test]
    fn saving_a_stale_draft_cannot_restore_a_disconnected_account() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut ui = Store::open(&paths)?;
        let mut worker = Store::open(&paths)?;
        let draft = ui.save_config(&Config {
            login: "alice".into(),
            oauth: true,
            read_org: true,
            ..Default::default()
        })?;
        worker.disconnect()?;
        let saved = ui.save_preferences(&draft)?;
        assert!(saved.login.is_empty());
        assert!(!saved.oauth);
        assert!(!saved.read_org);
        assert!(!saved.has_detail_token);
        assert_eq!(worker.ingest("alice", &[event()], false)?, 0);
        Ok(())
    }

    #[test]
    fn preferences_wait_for_an_account_transaction_before_reading_its_state() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut ui = Store::open(&paths)?;
        let draft = Config {
            interval_secs: 60,
            ..Default::default()
        };
        let mut worker = Store::open(&paths)?;
        let (send, receive) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || -> Result<()> {
            let transaction = worker
                .conn
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            let current = Config {
                login: "alice".into(),
                oauth: true,
                read_org: true,
                revision: 8,
                ..Default::default()
            };
            transaction.execute(
                "INSERT INTO kv(key,value) VALUES('config',?1)",
                [serde_json::to_string(&current)?],
            )?;
            send.send(())?;
            std::thread::sleep(StdDuration::from_millis(75));
            transaction.commit()?;
            Ok(())
        });
        receive.recv()?;
        let result = ui.save_preferences(&draft);
        writer.join().unwrap()?;
        let saved = result?;
        assert_eq!(saved.login, "alice");
        assert!(saved.oauth);
        assert!(saved.read_org);
        assert_eq!(saved.interval_secs, 60);
        assert_eq!(saved.revision, 9);
        Ok(())
    }

    #[test]
    fn simultaneous_pause_toggles_preserve_account_and_preferences() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut ui = Store::open(&paths)?;
        let initial = ui.save_config(&Config {
            login: "alice".into(),
            oauth: true,
            read_org: true,
            interval_secs: 300,
            show_preview: true,
            ..Default::default()
        })?;
        let mut tray = Store::open(&paths)?;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let worker_barrier = barrier.clone();
        let worker = std::thread::spawn(move || {
            worker_barrier.wait();
            tray.toggle_pause()
        });
        barrier.wait();
        let toggled = ui.toggle_pause();
        worker.join().unwrap()?;
        toggled?;
        let mut expected = initial.clone();
        expected.revision += 2;
        assert_eq!(ui.config()?, expected);
        assert!(ui.take_flag("refresh")?);
        Ok(())
    }

    #[test]
    fn legacy_token_config_remains_valid_when_saving_preferences() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut store = Store::open(&paths)?;
        store.put(
            "config",
            r#"{"login":"alice","has_detail_token":true,"read_org":true}"#,
        )?;
        let old = store.config()?;
        assert!(!old.oauth);
        let saved = store.save_preferences(&Config::default())?;
        assert_eq!(saved.login, "alice");
        assert!(saved.has_detail_token);
        assert!(saved.read_org);
        assert!(!saved.oauth);
        Ok(())
    }

    #[test]
    fn stale_account_snapshot_preserves_new_preferences_and_tray_pause() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        let mut ui = Store::open(&paths)?;
        let mut account_worker = Store::open(&paths)?;
        let mut account = account_worker.config()?;
        account.login = "alice".into();
        account.oauth = true;
        account.read_org = true;

        let mut preferences = ui.config()?;
        preferences.interval_secs = 300;
        preferences.show_preview = true;
        preferences.desktop_notifications = false;
        preferences.rules.mentions = false;
        preferences.repositories = vec![crate::model::Repo::parse("alice/project")?];
        preferences.manual_teams = vec![crate::model::Repo::parse("acme/team")?];
        ui.save_preferences(&preferences)?;
        let mut expected = ui.toggle_pause()?;
        expected.login = account.login.clone();
        expected.oauth = account.oauth;
        expected.read_org = account.read_org;
        expected.has_detail_token = account.has_detail_token;
        expected.revision += 1;

        ui.take_flag("refresh")?;
        assert_eq!(account_worker.save_account(&account)?, expected);
        assert_eq!(ui.config()?, expected);
        assert!(ui.take_flag("refresh")?);
        Ok(())
    }
}
