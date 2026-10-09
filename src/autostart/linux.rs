//! Native Linux uses the XDG autostart directory; Flatpak uses the portal.
use anyhow::{Context, Result, ensure};
use gio::{glib, prelude::*};
use std::{
    fs,
    path::{Path, PathBuf},
};

const FILE_NAME: &str = "io.hush.github.desktop";

pub(super) fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").is_file()
}

fn config_home() -> Result<PathBuf> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().to_owned())
        .context("No user configuration directory is available")
}

fn config_dirs() -> Vec<PathBuf> {
    let value = std::env::var_os("XDG_CONFIG_DIRS")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "/etc/xdg".into());
    std::env::split_paths(&value)
        .filter(|path| path.is_absolute())
        .collect()
}

pub(super) fn status() -> Result<bool> {
    read_status(&config_home()?, &config_dirs(), &std::env::current_exe()?)
}

fn read_status(home: &Path, system: &[PathBuf], executable: &Path) -> Result<bool> {
    for directory in std::iter::once(home).chain(system.iter().map(PathBuf::as_path)) {
        let path = directory.join("autostart").join(FILE_NAME);
        match fs::read_to_string(&path) {
            Ok(contents) => {
                let desktops = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
                return entry_enabled(&contents, executable, &desktops);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(false)
}

fn entry_enabled(contents: &str, executable: &Path, desktops: &str) -> Result<bool> {
    let entry = glib::KeyFile::new();
    entry.load_from_data(contents, glib::KeyFileFlags::NONE)?;
    if entry.boolean("Desktop Entry", "Hidden").unwrap_or(false)
        || entry
            .boolean("Desktop Entry", "X-GNOME-Autostart-enabled")
            .ok()
            == Some(false)
    {
        return Ok(false);
    }
    ensure!(
        entry.string("Desktop Entry", "Type")?.as_str() == "Application",
        "Invalid autostart entry type"
    );
    let args = glib::shell_parse_argv(entry.string("Desktop Entry", "Exec")?.as_str())?;
    if args.len() != 2
        || args[0].to_string_lossy().replace("%%", "%") != executable.to_string_lossy()
        || args[1] != "--tray"
    {
        return Ok(false);
    }
    if let Ok(list) = entry.string_list("Desktop Entry", "OnlyShowIn")
        && !list
            .iter()
            .any(|name| desktops.split(':').any(|desktop| desktop == name.as_str()))
    {
        return Ok(false);
    }
    if let Ok(list) = entry.string_list("Desktop Entry", "NotShowIn")
        && list
            .iter()
            .any(|name| desktops.split(':').any(|desktop| desktop == name.as_str()))
    {
        return Ok(false);
    }
    if let Ok(required) = entry.string("Desktop Entry", "TryExec")
        && !required.is_empty()
        && glib::find_program_in_path(required.as_str()).is_none()
    {
        return Ok(false);
    }
    Ok(true)
}

pub(super) fn set_enabled(enabled: bool) -> Result<bool> {
    let home = config_home()?;
    let executable = std::env::current_exe()?;
    write_entry(&home, &executable, enabled)?;
    read_status(&home, &config_dirs(), &executable)
}

fn desktop_entry(executable: &Path, enabled: bool) -> Result<String> {
    if !enabled {
        // A user override also disables any system-wide entry with the same ID.
        return Ok("[Desktop Entry]\nType=Application\nName=Hush\nHidden=true\n".into());
    }
    ensure!(
        executable.is_absolute(),
        "Autostart requires an absolute executable path"
    );
    let path = executable
        .to_str()
        .context("Executable path is not UTF-8")?;
    ensure!(
        !path.chars().any(|c| c.is_control() || c == '='),
        "Executable path cannot be represented in an XDG desktop entry"
    );
    // Exec quoting is parsed after the desktop file's string escapes. Escape
    // both layers, then escape percent field codes; no shell evaluates this.
    let mut quoted = String::from("\"");
    for character in path.chars() {
        match character {
            '\\' => quoted.push_str("\\\\\\\\"),
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            '%' => quoted.push_str("%%"),
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=Hush\nExec={quoted} --tray\nIcon=io.hush.github\nTerminal=false\nStartupNotify=false\nHidden=false\n"
    ))
}

fn write_entry(home: &Path, executable: &Path, enabled: bool) -> Result<()> {
    let contents = desktop_entry(executable, enabled)?;
    let directory = home.join("autostart");
    fs::create_dir_all(&directory)?;
    // Atomic replacement also replaces a symlink itself rather than its target.
    gio::File::for_path(directory.join(FILE_NAME)).replace_contents(
        contents.as_bytes(),
        None,
        false,
        gio::FileCreateFlags::PRIVATE | gio::FileCreateFlags::REPLACE_DESTINATION,
        None::<&gio::Cancellable>,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_login_entry_is_opt_in_and_disabling_overrides_system_entries() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let home = temp.path().join("user");
        let system = temp.path().join("system");
        let binary = Path::new("/opt/Hush app/hush");
        assert!(!read_status(&home, &[], binary)?);
        assert!(!home.exists());
        write_entry(&system, binary, true)?;
        assert!(read_status(&home, std::slice::from_ref(&system), binary)?);
        write_entry(&home, binary, false)?;
        assert!(!read_status(&home, std::slice::from_ref(&system), binary)?);
        assert!(read_status(&system, &[], binary)?);
        write_entry(&home, binary, true)?;
        assert!(read_status(&home, &[], binary)?);
        let contents = fs::read_to_string(home.join("autostart").join(FILE_NAME))?;
        assert!(contents.contains("Exec=\"/opt/Hush app/hush\" --tray\n"));
        write_entry(&home, binary, false)?;
        write_entry(&home, binary, false)?;
        assert!(!read_status(&home, &[], binary)?);
        Ok(())
    }

    #[test]
    fn desktop_command_round_trips_reserved_characters_without_shell_expansion() -> Result<()> {
        let path = Path::new("/opt/Hush $HOME `cmd` \\\" %f/hush");
        let data = desktop_entry(path, true)?;
        let entry = glib::KeyFile::new();
        entry.load_from_data(&data, glib::KeyFileFlags::NONE)?;
        let parsed = glib::shell_parse_argv(entry.string("Desktop Entry", "Exec")?.as_str())?;
        assert_eq!(parsed.len(), 2);
        assert_eq!(
            parsed[0].to_string_lossy().replace("%%", "%"),
            path.to_string_lossy()
        );
        assert_eq!(parsed[1], "--tray");
        assert!(desktop_entry(Path::new("/opt/hush\nExec=bad"), true).is_err());
        assert!(desktop_entry(Path::new("hush"), true).is_err());
        Ok(())
    }

    #[test]
    fn replacing_a_symlink_never_changes_its_target() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let target = temp.path().join("original.desktop");
        fs::write(&target, "unchanged")?;
        let home = temp.path().join("user");
        fs::create_dir_all(home.join("autostart"))?;
        std::os::unix::fs::symlink(&target, home.join("autostart").join(FILE_NAME))?;
        let binary = Path::new("/opt/hush");
        write_entry(&home, binary, false)?;
        assert_eq!(fs::read_to_string(target)?, "unchanged");
        assert!(!read_status(&home, &[], binary)?);
        Ok(())
    }

    #[test]
    fn stale_commands_and_desktop_disabled_entries_are_not_reported_enabled() -> Result<()> {
        let binary = Path::new("/opt/hush");
        let data = desktop_entry(binary, true)?;
        assert!(entry_enabled(&data, binary, "niri")?);
        assert!(!entry_enabled(&data, Path::new("/new/hush"), "niri")?);
        assert!(!entry_enabled(
            &format!("{data}OnlyShowIn=GNOME;\n"),
            binary,
            "niri"
        )?);
        assert!(entry_enabled(
            &format!("{data}OnlyShowIn=GNOME;\n"),
            binary,
            "GNOME:Unity"
        )?);
        assert!(!entry_enabled(
            &format!("{data}NotShowIn=niri;\n"),
            binary,
            "niri"
        )?);
        assert!(!entry_enabled(
            &format!("{data}X-GNOME-Autostart-enabled=false\n"),
            binary,
            "niri"
        )?);
        assert!(!entry_enabled(
            &format!("{data}TryExec=/nonexistent/hush-autostart-test\n"),
            binary,
            "niri"
        )?);
        assert!(!entry_enabled(
            &data.replace("--tray", "--demo"),
            binary,
            "niri"
        )?);
        Ok(())
    }
}
