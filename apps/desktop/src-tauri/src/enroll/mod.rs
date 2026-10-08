//! Voice enrollment (02-04).
//!
//! The user-facing path is "read a paragraph for 1–3 minutes so we can clone
//! your voice". Modules:
//!
//! - [`capture`] (T4.1) — real microphone capture, guard checks (duration,
//!   size, silence, speech), the startup trim, and the 16 kHz mono WAV export
//!   under `<app data>/enroll/`.
//! - [`register`] (T4.2) — the 火山 voice_clone training call.
//! - [`voice_store`] (T4.2/T4.3) — the profile on disk and the
//!   clone-or-preset resolution the cascade reads per segment.
//!
//! Privacy (T-02-16): the sample and the speaker id live only in the app data
//! directory, are never logged and never enter the session JSONL. Training is
//! the only outbound path and it points at exactly one host.

pub mod capture;
pub mod register;
pub mod voice_store;

use std::path::Path;

/// Creates (or truncates) a file owner-only — **at creation**, not by a chmod
/// afterwards (WR-07).
///
/// The enrollment artifacts are biometric data (T-02-16): the raw take and the
/// profile that names the user's voice id. `File::create`/`WavWriter::create`
/// apply the process umask (typically `0644` on macOS) and leave the file
/// readable by any local user or process until a later `set_permissions` runs
/// (CWE-377). This helper asks for `0600` on the open — so the file is born
/// private — and repairs an existing file's mode after the open but **before
/// the caller writes a single byte**, so the window the finding names cannot
/// exist on either path. Same pattern as the trace writer's `open_private`.
pub(crate) fn create_private(path: &Path) -> Result<std::fs::File, std::io::Error> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::os::unix::fs::PermissionsExt;

    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    // `mode` is consulted only when the open creates the file. A path that
    // already existed (a rerun, or an artifact written before this fix kept
    // its old mode) is tightened here — still before any new bytes land.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WR-07 mechanism pin. The window between "created with the umask" and "a
    /// later chmod" is invisible to any test that observes the final mode — a
    /// behavioral assertion sees `0600` either way — so the writers' shape is
    /// pinned at the source, the same way `trace` pins "no sockets".
    #[test]
    fn enrollment_artifacts_are_created_private_not_chmod_ed_afterwards() {
        let source = include_str!("mod.rs");
        assert!(
            source.contains("mode(0o600)"),
            "the private open must request 0600 at creation"
        );

        // `concat!` splits every needle so the scan cannot match its own
        // literals (the trace module's socket scan uses the same trick).
        let wav = include_str!("capture.rs");
        assert!(
            wav.contains(concat!("create_private(", "&path)")),
            "the take must be created through the private open"
        );
        assert!(
            !wav.contains(concat!("WavWriter::", "create")),
            "hound's create applies the umask: the take would be world-readable until a chmod ran"
        );

        let profile = include_str!("voice_store.rs");
        assert!(
            profile.contains(concat!("create_private(", "&path)")),
            "the profile must be created through the private open"
        );
        assert!(
            !profile.contains(concat!("File::", "create(")),
            "File::create applies the umask: the voice id would be world-readable until a chmod ran"
        );
    }

    /// The other half of the mechanism, behaviorally: a file that already
    /// exists with the umask's mode (a rerun, or an artifact from before the
    /// fix) is tightened by the open itself — callers never have to remember a
    /// chmod, which is exactly how the old window appeared.
    #[test]
    fn an_existing_artifact_is_tightened_before_anything_is_written() {
        use std::io::Write as _;
        use std::os::unix::fs::PermissionsExt as _;

        let dir = std::env::temp_dir().join(format!("nextalk-enroll-private-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("take.wav");

        std::fs::write(&path, b"old take").expect("pre-existing artifact");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("umask mode");

        let mut file = create_private(&path).expect("private open");
        let mode = std::fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "tightened by the open, not by a chmod later");
        file.write_all(b"new take").expect("write");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
