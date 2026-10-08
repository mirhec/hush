//! Regression coverage for worker/GUI/heartbeat concurrency. No credentials or HTTP.
use anyhow::Result;
use hush::{
    model::Config,
    storage::{Paths, Store},
};
use rusqlite::{Connection, TransactionBehavior};
use std::{sync::mpsc, thread, time::Duration};

#[test]
fn commands_and_settings_wait_for_a_concurrent_writer() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = Paths::at(temp.path().join("data"))?;
    let mut store = Store::open(&paths)?;
    store.request_refresh()?;
    // Simulate the heartbeat holding SQLite's write lock. A deferred
    // read-then-write transaction fails immediately despite busy_timeout.
    for settings in [false, true] {
        let mut writer = Connection::open(paths.db())?;
        let (tx, rx) = mpsc::channel();
        let worker = thread::spawn(move || -> Result<()> {
            let transaction = writer.transaction_with_behavior(TransactionBehavior::Immediate)?;
            transaction.execute("INSERT OR REPLACE INTO kv VALUES ('heartbeat','1')", [])?;
            tx.send(())?;
            thread::sleep(Duration::from_millis(150));
            transaction.commit()?;
            Ok(())
        });
        rx.recv()?;
        let result = if settings {
            store.save_config(&Config::default()).map(|_| ())
        } else {
            store.take_flag("refresh").map(|taken| assert!(taken))
        };
        worker.join().unwrap()?;
        result?;
    }
    Ok(())
}

#[test]
fn a_command_is_consumed_by_only_one_reader() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let paths = Paths::at(temp.path().join("data"))?;
    let mut first = Store::open(&paths)?;
    let mut second = Store::open(&paths)?;
    first.request_refresh()?;
    let reader = thread::spawn(move || second.take_flag("refresh"));
    let a = first.take_flag("refresh")?;
    let b = reader.join().unwrap()?;
    assert_ne!(a, b);
    assert!(!first.take_flag("refresh")?);
    Ok(())
}

#[cfg(target_os = "linux")]
mod process {
    use super::*;
    use hush::engine;
    use std::{
        process::{Command, Output},
        time::Instant,
    };

    struct Worker {
        _temp: tempfile::TempDir,
        paths: Paths,
    }
    impl Worker {
        fn new() -> Result<Self> {
            let temp = tempfile::tempdir()?;
            let paths = Paths::at(temp.path().join("hush"))?;
            Ok(Self { _temp: temp, paths })
        }
        fn command(&self, option: &str) -> Command {
            let mut command = Command::new(env!("CARGO_BIN_EXE_hush"));
            command.env("XDG_DATA_HOME", self._temp.path()).arg(option);
            command
        }
        fn run(&self, option: &str) -> Result<Output> {
            Ok(self.command(option).output()?)
        }
        fn stop(&self) -> Result<()> {
            assert!(self.run("--stop")?.status.success());
            let start = Instant::now();
            while engine::is_running(&self.paths)? {
                anyhow::ensure!(
                    start.elapsed() < Duration::from_secs(8),
                    "Worker did not stop: {:?}",
                    (
                        Store::open(&self.paths)?.status()?,
                        std::fs::read_to_string(self.paths.log_path())
                    )
                );
                thread::sleep(Duration::from_millis(30));
            }
            Ok(())
        }
    }
    impl Drop for Worker {
        fn drop(&mut self) {
            let _ = self.stop();
        }
    }

    #[test]
    fn launch_confirms_health_survives_writes_and_restarts() -> Result<()> {
        let worker = Worker::new()?;
        let started = worker.run("--start")?;
        assert!(
            started.status.success(),
            "{}",
            String::from_utf8_lossy(&started.stderr)
        );
        let mut store = Store::open(&worker.paths)?;
        assert!(engine::is_running(&worker.paths)?);
        let initial_heartbeat = store.status()?.heartbeat;
        assert!(store.status()?.alive());
        // A second --background invocation must exit without stopping the owner.
        assert!(worker.run("--background")?.status.success());
        assert!(engine::is_running(&worker.paths)?);
        // Exercise real heartbeat ticks alongside GUI writes and command handling.
        let until = Instant::now() + Duration::from_secs(6);
        while Instant::now() < until {
            let config = store.config()?;
            store.save_config(&config)?;
            store.request_refresh()?;
            thread::sleep(Duration::from_millis(20));
        }
        let status: serde_json::Value = serde_json::from_slice(&worker.run("--status")?.stdout)?;
        assert_eq!(status["running"], true);
        assert_eq!(status["healthy"], true);
        assert!(status["service_error"].is_null());
        assert!(
            store.status()?.heartbeat > initial_heartbeat,
            "Heartbeat stopped advancing"
        );
        worker
            .stop()
            .map_err(|e| anyhow::anyhow!("First stop: {e:#}"))?;
        assert!(!store.status()?.alive());
        // A stop issued while already stopped must not poison the next startup.
        store.request_stop()?;
        let mut child = worker.command("--background").spawn()?;
        let start = Instant::now();
        while !engine::is_running(&worker.paths)? || !store.status()?.alive() {
            assert!(
                child.try_wait()?.is_none(),
                "Background restart exited early"
            );
            assert!(start.elapsed() < Duration::from_secs(5));
            thread::sleep(Duration::from_millis(30));
        }
        worker
            .stop()
            .map_err(|e| anyhow::anyhow!("Restart stop: {e:#}"))?;
        assert!(child.wait()?.success());
        Ok(())
    }

    #[test]
    fn launch_reports_startup_failure_and_retains_the_cause() -> Result<()> {
        let worker = Worker::new()?;
        let store = Store::open(&worker.paths)?;
        store.put("config", "broken JSON")?;
        let result = worker.run("--start")?;
        assert!(!result.status.success());
        let error = String::from_utf8(result.stderr)?;
        assert!(error.contains("Einstellungen sind beschädigt"), "{error}");
        assert!(error.contains("service.log"));
        assert!(!engine::is_running(&worker.paths)?);
        assert!(!store.status()?.alive());
        assert!(
            store
                .status()?
                .service_error
                .unwrap()
                .contains("Einstellungen sind beschädigt")
        );
        let log = std::fs::read_to_string(worker.paths.log_path())?;
        assert!(log.contains("Einstellungen sind beschädigt"));
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(worker.paths.log_path())?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        Ok(())
    }
}
