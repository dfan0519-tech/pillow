//! Allow-listed child environment construction.

use std::collections::HashMap;
use std::path::Path;

use crate::config::GrokAcpConfig;
use crate::error::internal;
use keeper_agent_protocol::RuntimeError;

const ALLOWED_ENV_NAMES: &[&str] = &[
    "SYSTEMROOT",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "PATH",
    "TEMP",
    "TMP",
];

/// Sensitive name patterns (case-insensitive substring). Never copy these.
const SENSITIVE_ENV_NAME: &[&str] = &[
    "KEY",
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "CREDENTIAL",
    "AUTH",
    "OPENAI",
    "ANTHROPIC",
    "XAI",
    "VOLC",
    "MINIMAX",
    "MIMO",
];

const PROFILE_ENV_NAMES: &[&str] = &["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"];

/// Build a new environment map for the ACP child.
///
/// Does **not** inherit the parent wholesale. HOME/USERPROFILE are set only
/// when an isolated home is configured. Extra env from config is applied last
/// for test harness control (still filtered for sensitive names).
pub fn build_child_env(config: &GrokAcpConfig) -> Result<HashMap<String, String>, RuntimeError> {
    let mut env = HashMap::new();

    for (name, value) in std::env::vars() {
        let upper = name.to_ascii_uppercase();
        if upper == "HOME"
            || upper == "USERPROFILE"
            || upper == "APPDATA"
            || upper == "LOCALAPPDATA"
        {
            continue;
        }
        if !ALLOWED_ENV_NAMES.iter().any(|a| *a == upper) {
            continue;
        }
        if PROFILE_ENV_NAMES.contains(&upper.as_str()) || is_sensitive_name(&upper) {
            continue;
        }
        env.insert(name, value);
    }

    if let Some(home) = &config.honor_isolated_home {
        apply_isolated_home(&mut env, home)?;
    }

    for (name, value) in &config.extra_env {
        let upper = name.to_ascii_uppercase();
        if PROFILE_ENV_NAMES.contains(&upper.as_str()) || is_sensitive_name(&upper) {
            return Err(internal(format!(
                "refusing sensitive extra env name `{name}`"
            )));
        }
        env.insert(name.clone(), value.clone());
    }

    Ok(env)
}

fn apply_isolated_home(env: &mut HashMap<String, String>, home: &Path) -> Result<(), RuntimeError> {
    let home_str = path_to_string(home)?;
    env.insert("HOME".into(), home_str.clone());
    env.insert("USERPROFILE".into(), home_str);

    let roaming = home.join("AppData").join("Roaming");
    if roaming.is_dir() {
        env.insert("APPDATA".into(), path_to_string(&roaming)?);
    }
    let local = home.join("AppData").join("Local");
    if local.is_dir() {
        env.insert("LOCALAPPDATA".into(), path_to_string(&local)?);
    }
    Ok(())
}

fn path_to_string(path: &Path) -> Result<String, RuntimeError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| internal("path is not valid UTF-8"))
}

fn is_sensitive_name(upper: &str) -> bool {
    SENSITIVE_ENV_NAME.iter().any(|s| upper.contains(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn does_not_copy_sensitive_parent_env() {
        let _guard = env_lock().lock().unwrap();
        // SAFETY: tests serialize env mutation via env_lock.
        unsafe {
            std::env::set_var("XAI_API_KEY", "should-not-leak");
            std::env::set_var("PATH", "C:\\Windows\\System32");
        }
        let cfg = GrokAcpConfig::new("grok");
        let env = build_child_env(&cfg).expect("env");
        assert!(!env.keys().any(|k| k.to_ascii_uppercase().contains("XAI")));
        assert!(!env.values().any(|v| v.contains("should-not-leak")));
        assert!(env.keys().any(|k| k.eq_ignore_ascii_case("PATH")));
        unsafe {
            std::env::remove_var("XAI_API_KEY");
        }
    }

    #[test]
    fn isolated_home_sets_profile_vars() {
        let _guard = env_lock().lock().unwrap();
        let dir = std::env::temp_dir().join(format!("keeper-acp-home-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("AppData").join("Roaming")).unwrap();
        std::fs::create_dir_all(dir.join("AppData").join("Local")).unwrap();
        let mut cfg = GrokAcpConfig::new("grok");
        cfg.honor_isolated_home = Some(dir.clone());
        let env = build_child_env(&cfg).expect("env");
        assert_eq!(
            env.get("HOME").map(String::as_str),
            Some(dir.to_str().unwrap())
        );
        assert_eq!(
            env.get("USERPROFILE").map(String::as_str),
            Some(dir.to_str().unwrap())
        );
        assert!(env.contains_key("APPDATA"));
        assert!(env.contains_key("LOCALAPPDATA"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_sensitive_extra_env() {
        let mut cfg = GrokAcpConfig::new("grok");
        cfg.extra_env.push(("MY_SECRET_TOKEN".into(), "x".into()));
        assert!(build_child_env(&cfg).is_err());
    }

    #[test]
    fn rejects_profile_extra_env() {
        for name in [
            "HOME",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "MY_CREDENTIAL",
        ] {
            let mut cfg = GrokAcpConfig::new("grok");
            cfg.extra_env.push((name.into(), "C:\\escape".into()));
            assert!(build_child_env(&cfg).is_err(), "{name} must be rejected");
        }
    }
}
