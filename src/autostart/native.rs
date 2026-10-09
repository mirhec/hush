//! Per-user login registration. Changes affect the next login, never the running app.

#[cfg(target_os = "macos")]
pub(super) use macos::{set_enabled, status};
#[cfg(target_os = "windows")]
pub(super) use windows::{set_enabled, status};

#[cfg(any(target_os = "windows", test))]
fn windows_command(executable: &[u16]) -> anyhow::Result<Vec<u16>> {
    anyhow::ensure!(
        !executable.is_empty() && !executable.iter().any(|unit| matches!(unit, 0 | 34)),
        "The executable path cannot be used for login startup"
    );
    let mut command = Vec::with_capacity(executable.len() + 10);
    command.push(b'"' as u16);
    command.extend_from_slice(executable);
    command.extend("\" --tray".encode_utf16());
    // Microsoft documents a 260-character limit for Run key command lines.
    anyhow::ensure!(
        command.len() <= 260,
        "The login startup command is too long"
    );
    command.push(0);
    Ok(command)
}

#[cfg(target_os = "windows")]
mod windows {
    use anyhow::{Context, Result, bail};
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_SUCCESS},
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
            REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW,
            RegOpenKeyExW, RegSetValueExW,
        },
    };

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE_NAME: &str = "Hush";

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            // The handle was returned by a successful open/create and is owned here.
            unsafe { RegCloseKey(self.0) };
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn result(code: u32, action: &str) -> Result<()> {
        if code != ERROR_SUCCESS {
            return Err(std::io::Error::from_raw_os_error(code as i32)).context(action.to_owned());
        }
        Ok(())
    }

    fn expected_command() -> Result<Vec<u16>> {
        let path = std::env::current_exe().context("Cannot locate the Hush executable")?;
        super::windows_command(&path.as_os_str().encode_wide().collect::<Vec<_>>())
    }

    fn read_command() -> Result<Option<Vec<u16>>> {
        let key = wide(RUN_KEY);
        let name = wide(VALUE_NAME);
        // Retry a bounded number of times if another process changes the value between calls.
        for _ in 0..3 {
            let mut bytes = 0;
            let code = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    RRF_RT_REG_SZ,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    &mut bytes,
                )
            };
            if code == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            result(code, "Cannot read login startup registration")?;
            anyhow::ensure!(bytes <= 32_768, "The login startup entry is too large");
            let mut value = vec![0_u16; (bytes as usize).div_ceil(2) + 1];
            let mut capacity = (value.len() * 2) as u32;
            let code = unsafe {
                RegGetValueW(
                    HKEY_CURRENT_USER,
                    key.as_ptr(),
                    name.as_ptr(),
                    RRF_RT_REG_SZ,
                    ptr::null_mut(),
                    value.as_mut_ptr().cast(),
                    &mut capacity,
                )
            };
            if code == ERROR_FILE_NOT_FOUND {
                return Ok(None);
            }
            if code == ERROR_MORE_DATA {
                continue;
            }
            result(code, "Cannot read login startup registration")?;
            anyhow::ensure!(capacity % 2 == 0, "Invalid login startup entry");
            value.truncate(capacity as usize / 2);
            while value.last() == Some(&0) {
                value.pop();
            }
            value.push(0);
            return Ok(Some(value));
        }
        bail!("Login startup registration changed while reading it")
    }

    pub(crate) fn status() -> Result<bool> {
        let Some(command) = read_command()? else {
            return Ok(false);
        };
        Ok(command == expected_command()?)
    }

    pub(crate) fn set_enabled(enabled: bool) -> Result<bool> {
        let path = wide(RUN_KEY);
        let name = wide(VALUE_NAME);
        let mut handle = ptr::null_mut();
        if enabled {
            let command = expected_command()?;
            let code = unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    ptr::null(),
                    REG_OPTION_NON_VOLATILE,
                    KEY_QUERY_VALUE | KEY_SET_VALUE,
                    ptr::null(),
                    &mut handle,
                    ptr::null_mut(),
                )
            };
            result(code, "Cannot open login startup registration")?;
            let key = Key(handle);
            let code = unsafe {
                RegSetValueExW(
                    key.0,
                    name.as_ptr(),
                    0,
                    REG_SZ,
                    command.as_ptr().cast(),
                    (command.len() * 2) as u32,
                )
            };
            result(code, "Cannot enable login startup")?;
        } else {
            let code = unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    KEY_SET_VALUE,
                    &mut handle,
                )
            };
            if code == ERROR_FILE_NOT_FOUND {
                return Ok(false);
            }
            result(code, "Cannot open login startup registration")?;
            let key = Key(handle);
            let code = unsafe { RegDeleteValueW(key.0, name.as_ptr()) };
            if code != ERROR_FILE_NOT_FOUND {
                result(code, "Cannot disable login startup")?;
            }
        }
        let actual = status()?;
        anyhow::ensure!(
            actual == enabled,
            "Login startup registration changed while saving it"
        );
        Ok(actual)
    }
}

#[cfg(any(target_os = "macos", all(test, unix)))]
const AGENT_LABEL: &str = "io.hush.github.agent";

#[cfg(any(target_os = "macos", all(test, unix)))]
fn launch_agent(executable: &std::path::Path) -> anyhow::Result<String> {
    let path = executable
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("The executable path is not valid Unicode"))?;
    anyhow::ensure!(
        executable.is_absolute(),
        "The executable path must be absolute"
    );
    anyhow::ensure!(
        path.chars().all(|ch| matches!(ch, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}')),
        "The executable path cannot be represented in a property list"
    );
    let escaped = path
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
        .replace('\r', "&#13;");
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\"><dict>\n\
         <key>Label</key><string>{AGENT_LABEL}</string>\n\
         <key>ProgramArguments</key><array><string>{escaped}</string><string>--tray</string></array>\n\
         <key>RunAtLoad</key><true/>\n\
         <key>KeepAlive</key><false/>\n\
         <key>ProcessType</key><string>Interactive</string>\n\
         <key>LimitLoadToSessionType</key><string>Aqua</string>\n\
         </dict></plist>\n"
    ))
}

#[cfg(any(target_os = "macos", all(test, unix)))]
fn write_agent(path: &std::path::Path, content: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    use std::{fs, io::Write};
    let directory = path.parent().context("Invalid login startup file path")?;
    fs::create_dir_all(directory).context("Cannot create the login startup directory")?;
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary = directory.join(format!(
        ".{AGENT_LABEL}-{}-{suffix}.tmp",
        std::process::id()
    ));
    struct Temporary(std::path::PathBuf);
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.0);
        }
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .context("Cannot create the login startup file")?;
    let cleanup = Temporary(temporary);
    file.write_all(content.as_bytes())
        .context("Cannot write the login startup file")?;
    file.sync_all()
        .context("Cannot save the login startup file")?;
    drop(file);
    fs::rename(&cleanup.0, path).context("Cannot replace the login startup file")?;
    Ok(())
}

#[cfg(any(target_os = "macos", all(test, unix)))]
fn remove_agent(path: &std::path::Path) -> anyhow::Result<()> {
    use anyhow::Context;
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).context("Cannot disable login startup"),
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use anyhow::{Context, Result, bail};
    use core_foundation::{
        array::CFArray,
        base::{CFType, TCFType},
        boolean::CFBoolean,
        data::CFData,
        dictionary::CFDictionary,
        propertylist::{CFPropertyList, create_with_data, kCFPropertyListImmutable},
        string::CFString,
    };
    use std::{fs, path::PathBuf};

    fn agent_path() -> Result<PathBuf> {
        let base = directories::BaseDirs::new().context("Cannot locate the home directory")?;
        Ok(base
            .home_dir()
            .join("Library/LaunchAgents/io.hush.github.agent.plist"))
    }

    fn read_agent() -> Result<Option<CFDictionary>> {
        let data = match fs::read(agent_path()?) {
            Ok(data) => data,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error).context("Cannot read login startup registration"),
        };
        parse_agent(&data).map(Some)
    }

    fn parse_agent(data: &[u8]) -> Result<CFDictionary> {
        let (raw, _) = create_with_data(CFData::from_buffer(data), kCFPropertyListImmutable)
            .map_err(|error| anyhow::anyhow!("Invalid login startup property list: {error}"))?;
        // create_with_data returns an owned Core Foundation object; the wrapper releases it.
        let plist = unsafe { CFPropertyList::wrap_under_create_rule(raw) };
        let dictionary = plist
            .downcast::<CFDictionary>()
            .context("Invalid login startup property list")?;
        let label = value(&dictionary, "Label").and_then(|v| v.downcast::<CFString>());
        anyhow::ensure!(
            label.is_some_and(|v| v.to_string() == super::AGENT_LABEL),
            "The login startup file belongs to another application"
        );
        Ok(dictionary)
    }

    fn value(dictionary: &CFDictionary, key: &str) -> Option<CFType> {
        let key = CFString::new(key);
        dictionary.find(key.as_CFTypeRef()).map(|value| {
            // The dictionary retains the value; the wrapper takes its own retained reference.
            unsafe { CFType::wrap_under_get_rule(*value) }
        })
    }

    pub(crate) fn status() -> Result<bool> {
        let Some(dictionary) = read_agent()? else {
            return Ok(false);
        };
        let executable = std::env::current_exe().context("Cannot locate the Hush executable")?;
        agent_enabled(&dictionary, &executable)
    }

    fn flag(dictionary: &CFDictionary, key: &str) -> Result<bool> {
        value(dictionary, key)
            .map(|value| {
                value
                    .downcast::<CFBoolean>()
                    .map(bool::from)
                    .with_context(|| format!("Invalid {key} flag in login startup property list"))
            })
            .transpose()
            .map(|value| value.unwrap_or(false))
    }

    fn agent_enabled(dictionary: &CFDictionary, executable: &std::path::Path) -> Result<bool> {
        if !flag(dictionary, "RunAtLoad")? || flag(dictionary, "Disabled")? {
            return Ok(false);
        }
        if let Some(program) = value(dictionary, "Program") {
            let Some(program) = program.downcast::<CFString>() else {
                return Ok(false);
            };
            if program.to_string() != executable.to_string_lossy() {
                return Ok(false);
            }
        }
        let Some(arguments) =
            value(dictionary, "ProgramArguments").and_then(|value| value.downcast::<CFArray>())
        else {
            return Ok(false);
        };
        if arguments.len() != 2 {
            return Ok(false);
        }
        let arguments = arguments
            .iter()
            .map(|item| {
                unsafe { CFType::wrap_under_get_rule(*item) }
                    .downcast::<CFString>()
                    .map(|value| value.to_string())
            })
            .collect::<Option<Vec<_>>>();
        let Some(arguments) = arguments else {
            return Ok(false);
        };
        Ok(arguments[0] == executable.to_string_lossy() && arguments[1] == "--tray")
    }

    pub(crate) fn set_enabled(enabled: bool) -> Result<bool> {
        let path = agent_path()?;
        // Validate ownership before replacing or removing an existing registration.
        read_agent()?;
        if enabled {
            let executable =
                std::env::current_exe().context("Cannot locate the Hush executable")?;
            super::write_agent(&path, &super::launch_agent(&executable)?)?;
        } else {
            super::remove_agent(&path)?;
        }
        let actual = status()?;
        if actual != enabled {
            bail!("Login startup registration changed while saving it");
        }
        Ok(actual)
    }

    #[cfg(test)]
    #[test]
    fn property_list_status_honors_native_types_and_disabled_flags() {
        let path = std::path::Path::new("/Applications/日本語 & Tools.app/Contents/MacOS/hush");
        let text = super::launch_agent(path).unwrap();
        let dictionary = parse_agent(text.as_bytes()).unwrap();
        assert!(agent_enabled(&dictionary, path).unwrap());
        assert!(!agent_enabled(&dictionary, std::path::Path::new("/other/hush")).unwrap());
        for text in [
            text.replace(
                "<key>RunAtLoad</key><true/>",
                "<key>RunAtLoad</key><false/>",
            ),
            text.replace("</dict>", "<key>Disabled</key><true/></dict>"),
        ] {
            assert!(!agent_enabled(&parse_agent(text.as_bytes()).unwrap(), path).unwrap());
        }
        let invalid = text.replace("</dict>", "<key>Disabled</key><string>true</string></dict>");
        assert!(agent_enabled(&parse_agent(invalid.as_bytes()).unwrap(), path).is_err());
        assert!(parse_agent(text.replace(super::AGENT_LABEL, "another.agent").as_bytes()).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::path::Path;

    #[test]
    fn windows_run_command_quotes_paths_and_preserves_unicode() {
        let path = r"C:\Program Files\日本語 & Tools\hush.exe";
        let command = windows_command(&path.encode_utf16().collect::<Vec<_>>()).unwrap();
        assert_eq!(command.last(), Some(&0));
        assert_eq!(
            String::from_utf16(&command[..command.len() - 1]).unwrap(),
            format!("\"{path}\" --tray")
        );
        assert!(windows_command(&[65, 0, 66]).is_err());
        assert!(windows_command(&[65, 34, 66]).is_err());
        assert!(windows_command(&vec![65; 260]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn launch_agent_escapes_paths_and_starts_only_in_a_login_session() {
        let path =
            Path::new("/Applications/Hush & <Tools> \"日本語\" 'Test'.app/Contents/MacOS/hush");
        let plist = launch_agent(path).unwrap();
        assert!(plist.contains("Hush &amp; &lt;Tools&gt; &quot;日本語&quot; &apos;Test&apos;.app"));
        assert!(plist.contains("<string>--tray</string>"));
        assert!(plist.contains("<key>RunAtLoad</key><true/>"));
        assert!(plist.contains("<key>KeepAlive</key><false/>"));
        assert!(plist.contains("<key>LimitLoadToSessionType</key><string>Aqua</string>"));
        assert!(launch_agent(Path::new("relative/hush")).is_err());
        assert!(launch_agent(Path::new("/tmp/bad\u{1}/hush")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn launch_agent_updates_atomically_and_removes_only_its_own_file() {
        use std::{fs, os::unix::fs::PermissionsExt};
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("LaunchAgents/io.hush.github.agent.plist");
        let first = launch_agent(Path::new("/Applications/Hush.app/Contents/MacOS/hush")).unwrap();
        write_agent(&path, &first).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), first);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let neighbor = path.with_file_name("another.agent.plist");
        fs::write(&neighbor, "untouched").unwrap();
        let second = launch_agent(Path::new(
            "/Users/test/Applications/Hush.app/Contents/MacOS/hush",
        ))
        .unwrap();
        write_agent(&path, &second).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), second);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 2);
        remove_agent(&path).unwrap();
        remove_agent(&path).unwrap();
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(neighbor).unwrap(), "untouched");
    }
}
