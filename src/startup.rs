//! Visible diagnostics for Windows GUI processes, which have no console.
use hush::storage::Paths;
use std::{io::Write, path::PathBuf};

pub fn is_gui_launch(option: Option<&str>) -> bool {
    matches!(option, None | Some("--tray" | "--window" | "--demo"))
}

fn write_report(paths: &Paths, error: &str) -> anyhow::Result<PathBuf> {
    let mut file = paths.startup_log()?;
    writeln!(
        file,
        "{} Hush {}\n{error}",
        chrono::Utc::now().to_rfc3339(),
        env!("CARGO_PKG_VERSION")
    )?;
    file.flush()?;
    Ok(paths.root.join("startup.log"))
}

#[cfg(target_os = "windows")]
pub fn report(error: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    let log = match Paths::discover().and_then(|paths| write_report(&paths, error)) {
        Ok(path) => format!("Details saved to:\n{}", path.display()),
        Err(failure) => format!("Could not save the error log: {failure:#}"),
    };
    let text: Vec<u16> = format!("Hush could not start.\n\n{error}\n\n{log}")
        .replace('\0', "")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    // Both strings are NUL-terminated and remain alive until the dialog closes.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            windows_sys::w!("Hush - Startup error"),
            MB_OK | MB_ICONERROR,
        );
    }
}

#[cfg(target_os = "windows")]
pub fn install_panic_hook() {
    let main_thread = std::thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        if std::thread::current().id() == main_thread {
            report(&info.to_string());
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_and_script_commands_never_open_error_dialogs() {
        for option in [
            "--background",
            "--start",
            "--status",
            "--stop",
            "--help",
            "--version",
            "--invalid",
        ] {
            assert!(!is_gui_launch(Some(option)), "{option}");
        }
        for option in [None, Some("--tray"), Some("--window"), Some("--demo")] {
            assert!(is_gui_launch(option));
        }
    }

    #[test]
    fn failure_report_survives_without_a_database_and_replaces_old_error() -> anyhow::Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::at(temp.path().join("data"))?;
        write_report(&paths, "first error with a much longer description")?;
        let path = write_report(&paths, "DX12 unavailable")?;
        let report = std::fs::read_to_string(path)?;
        assert!(report.contains("DX12 unavailable"));
        assert!(!report.contains("first error"));
        assert!(!paths.db().exists());
        Ok(())
    }
}
