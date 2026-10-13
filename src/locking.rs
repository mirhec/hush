//! Interpret fs2 contention consistently on Unix and Windows.
use std::io::{Error, ErrorKind};

pub fn is_contended(error: &Error) -> bool {
    // Windows LockFileEx returns ERROR_LOCK_VIOLATION (33), whose Rust error
    // kind is not WouldBlock. Compare the library's platform-specific OS code,
    // not its broad ErrorKind: unrelated I/O failures must remain failures.
    error.kind() == ErrorKind::WouldBlock
        || error
            .raw_os_error()
            .is_some_and(|code| Some(code) == fs2::lock_contended_error().raw_os_error())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs2::FileExt;

    #[test]
    fn occupied_os_lock_is_contention_and_releases_with_its_owner() -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let owner = file.reopen()?;
        let contender = file.reopen()?;
        owner.try_lock_exclusive()?;
        let error = contender.try_lock_exclusive().unwrap_err();
        assert!(is_contended(&error), "{error:?}");
        assert_eq!(
            error.raw_os_error(),
            fs2::lock_contended_error().raw_os_error()
        );
        drop(owner);
        contender.try_lock_exclusive()?;
        Ok(())
    }

    #[test]
    fn permission_sharing_and_other_io_failures_are_not_contention() {
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::NotFound,
            ErrorKind::Other,
        ] {
            assert!(!is_contended(&Error::new(kind, "test error")));
        }
        assert!(is_contended(&fs2::lock_contended_error()));
        #[cfg(windows)]
        {
            assert!(is_contended(&Error::from_raw_os_error(33)));
            assert!(!is_contended(&Error::from_raw_os_error(32)));
            assert!(!is_contended(&Error::from_raw_os_error(5)));
        }
    }
}
