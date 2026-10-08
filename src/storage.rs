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
        let mut next = config.clone();
        next.validate()?;
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
        next.revision = previous.revision.saturating_add(1);
        tx.execute("INSERT INTO kv(key,value) VALUES('config',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [serde_json::to_string(&next)?])?;
        tx.commit()?;
        self.put("refresh", "1")?;
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
            let (s, unread, archived) = r?;
            let mut e: Event = serde_json::from_str(&s)?;
            e.unread = unread;
            e.archived = archived;
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
    pub fn archive(&self, id: &str, value: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE events SET archived=?2,unread=0,notified=1 WHERE id=?1",
            params![id, value],
        )?;
        Ok(())
    }
    pub fn read_all(&self) -> Result<()> {
        self.conn
            .execute("UPDATE events SET unread=0 WHERE archived=0", [])?;
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
        tx.execute_batch("DELETE FROM events; DELETE FROM tasks; DELETE FROM seen; DELETE FROM kv WHERE key LIKE 'cursor:%'; DELETE FROM kv WHERE key='config';")?;
        tx.commit()?;
        self.put("refresh", "1")?;
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
            archived: false,
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
}
