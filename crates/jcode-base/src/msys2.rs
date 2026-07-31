//! MSYS2 path-system support for jcode on Windows.
//!
//! jcode runs as a native Windows process, so the session working directory is
//! stored as a native path such as `C:\msys64\home\cornw\jcode`. When commands
//! are executed through MSYS2 bash (see [`crate::config::ToolConfig::shell_command`])
//! the agent thinks and writes in MSYS2 paths, which MSYS2 maps through its
//! mount table (e.g. that same directory is `/home/cornw/jcode`, *not*
//! `/c/msys64/home/cornw/jcode`). Only `cygpath` knows the mount table, so
//! native-Windows <-> MSYS2 conversion must go through `cygpath` to be correct;
//! the pure-string helpers here are best-effort fallbacks for when `cygpath` is
//! not on `PATH`.
//!
//! The module also resolves the effective shell jcode should run commands with
//! on Windows: an explicitly configured `shell_command` wins, otherwise a
//! detected MSYS2 bash is used (so jcode "defaults to the MSYS2 path system"
//! when MSYS2 is present), otherwise `cmd.exe`.

use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::process::Command;

/// Common MSYS2 install roots searched (in priority order) when locating bash.
#[cfg(windows)]
const CANDIDATE_MSYS_ROOTS: &[&str] = &["C:\\msys64", "C:\\msys2"];

/// Best-effort: infer an MSYS2 root from a native Windows path that lives under
/// an MSYS2 install (e.g. `C:\msys64\home\...` -> `C:\msys64`).
#[cfg(windows)]
fn msys_root_from_native_path(path: &Path) -> Option<PathBuf> {
    let text = path.to_string_lossy();
    for root in CANDIDATE_MSYS_ROOTS {
        let prefix = format!("{}\\", root);
        if text.starts_with(&prefix) || text.eq_ignore_ascii_case(root) {
            return Some(PathBuf::from(*root));
        }
    }
    None
}

#[cfg(windows)]
fn candidate_roots() -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Ok(root) = std::env::var("MSYS2_ROOT") {
        let root = root.trim().trim_matches('"');
        if !root.is_empty() {
            roots.push(PathBuf::from(root));
        }
    }
    roots.extend(CANDIDATE_MSYS_ROOTS.iter().map(|root| PathBuf::from(*root)));
    if let Ok(cwd) = std::env::current_dir()
        && let Some(root) = msys_root_from_native_path(&cwd)
    {
        roots.push(root);
    }
    roots
}

/// Locate an MSYS2 `bash.exe`. Returns `None` on non-Windows or when no MSYS2
/// installation can be found. Checks `<root>\usr\bin\bash.exe` for each known
/// root (plus `MSYS2_ROOT`), then falls back to searching `PATH`.
#[cfg(windows)]
pub fn find_msys2_bash() -> Option<PathBuf> {
    for root in candidate_roots() {
        let bash = root.join("usr").join("bin").join("bash.exe");
        if bash.is_file() {
            return Some(bash);
        }
    }
    find_msys2_bash_on_path()
}

/// Non-Windows builds never run through MSYS2 bash by default.
#[cfg(not(windows))]
pub fn find_msys2_bash() -> Option<PathBuf> {
    None
}

/// Locate an MSYS-style `bash.exe` already on `PATH`, preferring one that sits
/// inside a known MSYS2 root.
#[cfg(windows)]
fn find_msys2_bash_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let mut fallback: Option<PathBuf> = None;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join("bash.exe");
        if candidate.is_file() {
            if msys_root_from_native_path(&candidate).is_some() {
                return Some(candidate);
            }
            fallback.get_or_insert(candidate);
        }
    }
    fallback
}

/// Locate `cygpath.exe` relative to a known MSYS2 root, or on `PATH`.
#[cfg(windows)]
fn find_cygpath() -> Option<PathBuf> {
    for root in candidate_roots() {
        let cygpath = root.join("usr").join("bin").join("cygpath.exe");
        if cygpath.is_file() {
            return Some(cygpath);
        }
    }
    std::env::var_os("PATH").and_then(|path| {
        std::env::split_paths(&path)
            .map(|dir| dir.join("cygpath.exe"))
            .find(|candidate| candidate.is_file())
    })
}

/// The effective shell to run commands with. On Windows, an explicitly set
/// `configured` shell wins; otherwise a detected MSYS2 bash is used (making
/// MSYS2 the default when present); otherwise `None` (the caller falls back to
/// `cmd.exe`). On non-Windows this always returns `None`.
///
/// The returned value is a native path to an executable, suitable for
/// `std::process::Command`.
pub fn resolve_shell_command(configured: Option<&str>) -> Option<String> {
    #[cfg(windows)]
    {
        if let Some(shell) = configured {
            let shell = shell.trim();
            if !shell.is_empty() {
                return Some(shell.to_string());
            }
        }
        find_msys2_bash().map(|p| p.to_string_lossy().into_owned())
    }
    #[cfg(not(windows))]
    {
        let _ = configured;
        None
    }
}

/// Convert a native Windows path into its MSYS2 (Unix) form using `cygpath -u`,
/// which resolves MSYS2 mount points correctly (e.g. `C:\msys64\home\cornw` ->
/// `/home/cornw`). Falls back to a pure-string conversion when `cygpath` is not
/// available. On non-Windows native paths are returned unchanged (they are
/// already in Unix form).
pub fn to_msys_path(path: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        let native = path.to_string_lossy();
        if let Some(cygpath) = find_cygpath() {
            let output = Command::new(&cygpath)
                .arg("-u")
                .arg(native.as_ref())
                .output()
                .ok()?;
            if output.status.success() {
                let converted = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !converted.is_empty() {
                    return Some(converted);
                }
            }
        }
        windows_to_msys_fallback(&native)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Some(path.to_string_lossy().into_owned())
    }
}

/// Convert an MSYS2 (Unix) path back into a native Windows path using
/// `cygpath -w`. Falls back to a pure-string conversion for the common
/// `/drive/...` and `/home/...` forms.
pub fn to_windows_path(msys: &str) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        if let Some(cygpath) = find_cygpath() {
            let output = Command::new(&cygpath).arg("-w").arg(msys).output().ok()?;
            if output.status.success() {
                let converted = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !converted.is_empty() {
                    return Some(PathBuf::from(converted));
                }
            }
        }
        msys_to_windows_fallback(msys)
    }
    #[cfg(not(windows))]
    {
        let _ = msys;
        Some(PathBuf::from(msys))
    }
}

/// Best-effort pure-string conversion of a native Windows path to the MSYS2
/// `/<drive>/<rest>` ("cygdrive") form: `C:\msys64\home` -> `/c/msys64/home`.
/// This is only a fallback; it does not know MSYS2 mount points (e.g. `/home`)
/// so the `cygpath` path should be preferred whenever possible.
#[cfg(windows)]
fn windows_to_msys_fallback(native: &str) -> Option<String> {
    let text = native.replace('\\', "/");
    let bytes = text.as_bytes();
    // `X:/...` or `X:...`
    if text.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'/' || text.len() == 2)
    {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = if text.len() >= 3 { &text[2..] } else { "" };
        return Some(format!("/{}{}", drive, rest));
    }
    // `//server/share/...` (UNC)
    if text.starts_with("//") {
        return Some(text);
    }
    None
}

/// Best-effort pure-string conversion of the MSYS2 `/<drive>/<rest>` form back
/// to native Windows: `/c/msys64/home` -> `C:\msys64\home`. Also handles
/// `//server/share` UNC paths. Returns `None` for paths it cannot map.
#[cfg(windows)]
fn msys_to_windows_fallback(msys: &str) -> Option<PathBuf> {
    let text = msys.trim();
    if let Some(rest) = text.strip_prefix("//") {
        // UNC: //server/share -> \\server\share
        return Some(PathBuf::from(format!("\\\\{}", rest.replace('/', "\\"))));
    }
    let text = text.strip_prefix('/')?;
    let bytes = text.as_bytes();
    if text.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b'/' {
        let drive = (bytes[0] as char).to_ascii_uppercase();
        let rest = &text[2..];
        return Some(PathBuf::from(format!(
            "{}:\\{}",
            drive,
            rest.replace('/', "\\")
        )));
    }
    None
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn fallback_windows_to_msys_cygdrive_form() {
        assert_eq!(
            windows_to_msys_fallback("C:\\msys64\\home\\cornw\\jcode").as_deref(),
            Some("/c/msys64/home/cornw/jcode")
        );
        assert_eq!(
            windows_to_msys_fallback("D:/work/project").as_deref(),
            Some("/d/work/project")
        );
        // UNC passes through
        assert_eq!(
            windows_to_msys_fallback("\\\\server\\share").as_deref(),
            Some("//server/share")
        );
        // Not a drive path
        assert_eq!(windows_to_msys_fallback("relative/path"), None);
    }

    #[test]
    fn fallback_msys_to_windows_roundtrip() {
        assert_eq!(
            msys_to_windows_fallback("/c/msys64/home/cornw/jcode")
                .map(|p| p.to_string_lossy().into_owned())
                .as_deref(),
            Some("C:\\msys64\\home\\cornw\\jcode")
        );
        assert_eq!(
            msys_to_windows_fallback("//server/share")
                .map(|p| p.to_string_lossy().into_owned())
                .as_deref(),
            Some("\\\\server\\share")
        );
        assert_eq!(msys_to_windows_fallback("/not-a-drive"), None);
    }

    #[test]
    fn resolve_prefers_configured_shell() {
        assert_eq!(
            resolve_shell_command(Some("C:\\custom\\shell.exe")).as_deref(),
            Some("C:\\custom\\shell.exe")
        );
        // Empty/whitespace configured value falls back to auto-detect.
        assert_eq!(
            resolve_shell_command(Some("   ")),
            resolve_shell_command(None)
        );
    }

    #[test]
    fn cygpath_roundtrip_when_available() {
        // Only meaningful when cygpath is installed; otherwise skip.
        if find_cygpath().is_none() {
            return;
        }
        let probe = windows_to_msys_fallback("C:\\msys64\\home\\cornw\\jcode").unwrap();
        let windows = to_windows_path(&probe).expect("convert back");
        // Native form round-trips through the fallback, so confirm cygpath
        // produced a path the OS can represent.
        assert!(windows.as_os_str().to_string_lossy().contains("msys64"));
    }
}
