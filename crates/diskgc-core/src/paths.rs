//! Where diskgc keeps its files. XDG-style on Unix (with the macOS log convention),
//! `%APPDATA%` / `%LOCALAPPDATA%` on Windows. Every path can be overridden with an
//! environment variable, which is what the tests use.

use std::path::PathBuf;

pub fn home() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// `~/.config/diskgc` (Unix) or `%APPDATA%\diskgc` (Windows). Override: `DISKGC_CONFIG_DIR`.
pub fn config_dir() -> PathBuf {
    if let Some(p) = env_path("DISKGC_CONFIG_DIR") {
        return p;
    }
    if cfg!(windows) {
        dirs::config_dir().unwrap_or_else(home).join("diskgc")
    } else {
        env_path("XDG_CONFIG_HOME")
            .unwrap_or_else(|| home().join(".config"))
            .join("diskgc")
    }
}

pub fn config_file() -> PathBuf {
    config_dir().join("config.toml")
}

/// `~/.local/share/diskgc` (Unix) or `%LOCALAPPDATA%\diskgc` (Windows). Override: `DISKGC_DATA_DIR`.
pub fn data_dir() -> PathBuf {
    if let Some(p) = env_path("DISKGC_DATA_DIR") {
        return p;
    }
    if cfg!(windows) {
        dirs::data_local_dir().unwrap_or_else(home).join("diskgc")
    } else {
        env_path("XDG_DATA_HOME")
            .unwrap_or_else(|| home().join(".local/share"))
            .join("diskgc")
    }
}

pub fn history_file() -> PathBuf {
    data_dir().join("history.jsonl")
}

/// `~/Library/Logs/diskgc.log` on macOS, `$XDG_STATE_HOME/diskgc/diskgc.log` on Linux,
/// `%LOCALAPPDATA%\diskgc\diskgc.log` on Windows. Override: `DISKGC_LOG_FILE`.
pub fn log_file() -> PathBuf {
    if let Some(p) = env_path("DISKGC_LOG_FILE") {
        return p;
    }
    if cfg!(target_os = "macos") {
        home().join("Library/Logs/diskgc.log")
    } else if cfg!(windows) {
        data_dir().join("diskgc.log")
    } else {
        env_path("XDG_STATE_HOME")
            .unwrap_or_else(|| home().join(".local/state"))
            .join("diskgc/diskgc.log")
    }
}

/// Directory for the IPC endpoint (socket file or port/token file). Override: `DISKGC_RUNTIME_DIR`.
pub fn runtime_dir() -> PathBuf {
    if let Some(p) = env_path("DISKGC_RUNTIME_DIR") {
        return p;
    }
    if cfg!(unix) {
        if let Some(p) = env_path("XDG_RUNTIME_DIR") {
            return p.join("diskgc");
        }
    }
    data_dir()
}

/// Unix socket path (Unix) or the file holding `port\ntoken` (Windows).
pub fn ipc_endpoint() -> PathBuf {
    if cfg!(windows) {
        runtime_dir().join("diskgc.ipc")
    } else {
        runtime_dir().join("diskgc.sock")
    }
}

/// The volume whose free space the daemon watches.
pub fn watched_volume() -> PathBuf {
    if let Some(p) = env_path("DISKGC_VOLUME") {
        return p;
    }
    if cfg!(target_os = "macos") {
        PathBuf::from("/System/Volumes/Data")
    } else {
        home()
    }
}

/// Expand a leading `~` and environment variables of the form `$NAME` / `${NAME}` / `%NAME%`.
pub fn expand(p: &str) -> PathBuf {
    let mut s = p.to_string();
    if let Some(rest) = s.strip_prefix('~') {
        s = format!("{}{}", home().display(), rest);
    }
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '$' => {
                let braced = chars.peek() == Some(&'{');
                if braced {
                    chars.next();
                }
                let mut name = String::new();
                while let Some(&n) = chars.peek() {
                    if n.is_alphanumeric() || n == '_' {
                        name.push(n);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if braced {
                    chars.next();
                }
                out.push_str(&std::env::var(&name).unwrap_or_default());
            }
            '%' if cfg!(windows) => {
                let mut name = String::new();
                for n in chars.by_ref() {
                    if n == '%' {
                        break;
                    }
                    name.push(n);
                }
                out.push_str(&std::env::var(&name).unwrap_or_default());
            }
            _ => out.push(c),
        }
    }
    PathBuf::from(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_home_and_env() {
        std::env::set_var("DISKGC_TEST_VAR", "abc");
        assert_eq!(expand("~/x"), home().join("x"));
        assert_eq!(
            expand("/tmp/$DISKGC_TEST_VAR/y"),
            PathBuf::from("/tmp/abc/y")
        );
        assert_eq!(
            expand("/tmp/${DISKGC_TEST_VAR}y"),
            PathBuf::from("/tmp/abcy")
        );
    }
}
