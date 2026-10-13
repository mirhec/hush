//! Run the real worker startup protocol on every OS, using only temporary data.
//! This executable handles --background so engine::launch can spawn itself as
//! usual without adding a data-directory override to the production application.
use anyhow::{Context, Result, ensure};
use hush::{
    engine,
    storage::{Paths, Store},
};
use std::{
    process::Command,
    thread,
    time::{Duration, Instant},
};

const TEST_ROOT: &str = "HUSH_WORKER_TEST_ROOT";

fn main() {
    if let Err(error) = run() {
        eprintln!("Worker lifecycle test: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match std::env::args().nth(1).as_deref() {
        Some("--background") => engine::run_background(test_paths()?),
        Some("--exercise") => exercise(test_paths()?),
        _ => {
            let temp = tempfile::tempdir()?;
            // Set the fixture directory only on child commands, never in the
            // process-global environment of a multithreaded Rust test runner.
            let mut child = Command::new(std::env::current_exe()?)
                .arg("--exercise")
                .env(TEST_ROOT, temp.path().join("data"))
                .spawn()?;
            let deadline = Instant::now() + Duration::from_secs(45);
            let status = loop {
                if let Some(status) = child.try_wait()? {
                    break status;
                }
                if Instant::now() >= deadline {
                    if let Ok(paths) = Paths::at(temp.path().join("data")) {
                        let _ = stop(&paths);
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!("Worker test timed out");
                }
                thread::sleep(Duration::from_millis(50));
            };
            ensure!(
                status.success(),
                "Worker lifecycle subprocess failed: {status}"
            );
            Ok(())
        }
    }
}

fn test_paths() -> Result<Paths> {
    Paths::at(
        std::env::var_os(TEST_ROOT)
            .context("Missing isolated test directory")?
            .into(),
    )
}

fn stop(paths: &Paths) -> Result<()> {
    Store::open(paths)?.request_stop()?;
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if matches!(engine::is_running(paths), Ok(false)) {
            return Ok(());
        }
        ensure!(Instant::now() < deadline, "Worker did not release its lock");
        thread::sleep(Duration::from_millis(30));
    }
}

struct Cleanup(Paths);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = stop(&self.0);
    }
}

fn exercise(paths: Paths) -> Result<()> {
    let _cleanup = Cleanup(paths.clone());
    let store = Store::open(&paths)?;
    ensure!(
        store.config()?.login.is_empty(),
        "Test must never use an account"
    );
    engine::launch(&paths).context("Initial worker launch")?;
    ensure!(
        engine::is_running(&paths)? && store.status()?.alive(),
        "Worker must report healthy"
    );
    let heartbeat = store.status()?.heartbeat;
    println!("Initial startup and lock-based status: OK");

    engine::launch(&paths).context("Repeated worker launch")?;
    let duplicate = Command::new(std::env::current_exe()?)
        .arg("--background")
        .output()?;
    ensure!(
        duplicate.status.success(),
        "Duplicate worker: {}",
        String::from_utf8_lossy(&duplicate.stderr)
    );
    ensure!(
        engine::is_running(&paths)? && store.status()?.alive(),
        "Duplicate stopped the existing worker"
    );
    let deadline = Instant::now() + Duration::from_secs(8);
    while store.status()?.heartbeat <= heartbeat {
        ensure!(Instant::now() < deadline, "Heartbeat stopped advancing");
        thread::sleep(Duration::from_millis(50));
    }
    println!("Repeated start, duplicate worker and continuing heartbeat: OK");

    stop(&paths)?;
    ensure!(
        !store.status()?.alive(),
        "Stopped worker still reports alive"
    );
    // A stale stop request must be cleared by the next real lock owner.
    store.request_stop()?;
    engine::launch(&paths).context("Worker restart")?;
    ensure!(
        engine::is_running(&paths)? && store.status()?.alive(),
        "Restart did not become healthy"
    );
    stop(&paths)?;
    println!("Stop and restart: OK");
    println!("Worker lifecycle: OK");
    Ok(())
}
