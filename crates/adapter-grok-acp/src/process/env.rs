//! Allow-listed child environment construction.

use std::collections::HashMap;
use std::path::Path;

use crate::config::GrokAcpConfig;
use crate::error::internal;
use keeper_agent_protocol::RuntimeError;
use keeper_process_executor::sensitive_name;

const ALLOWED_ENV_NAMES: &[&str] = &[
    "SYSTEMROOT",
    "WINDIR",
    "COMSPEC",
    "PATHEXT",
    "PATH",
    "TEMP",
    "TMP",
];

const PROFILE_ENV_NAMES: &[&str] = &["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"];

/// Build a new environment map for the ACP child.
///
/// Does **not** inherit the parent wholesale. In login mode, current profile
/// variables are copied through the allowlist without redirecting them. In
/// API-key mode, an isolated home replaces them. Extra env remains test-only
/// and is always filtered for sensitive names; secrets use a separate typed
/// spawn channel and never enter this map.
pub fn build_child_env(config: &GrokAcpConfig) -> Result<HashMap<String, String>, RuntimeError> {
    let mut env = HashMap::new();

    for (name, value) in std::env::vars() {
        let upper = name.to_ascii_uppercase();
        if upper == "HOME"
            || upper == "USERPROFILE"
            || upper == "APPDATA"
            || upper == "LOCALAPPDATA"
        {
            if config.honor_isolated_home.is_none() {
                env.insert(name, value);
            }
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
            return Err(internal("acp_extra_env_rejected"));
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
    sensitive_name(std::ffi::OsStr::new(upper))
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

    #[test]
    fn login_mode_preserves_current_profile_without_a_home_redirect() {
        let _guard = env_lock().lock().unwrap();
        let previous_home = std::env::var_os("HOME");
        let previous_profile = std::env::var_os("USERPROFILE");
        // SAFETY: tests serialize env mutation via env_lock.
        unsafe {
            std::env::set_var("HOME", "C:\\Users\\current");
            std::env::set_var("USERPROFILE", "C:\\Users\\current");
        }
        let cfg = GrokAcpConfig::new("grok");
        let env = build_child_env(&cfg).expect("env");
        assert_eq!(
            env.get("HOME").map(String::as_str),
            Some("C:\\Users\\current")
        );
        assert_eq!(
            env.get("USERPROFILE").map(String::as_str),
            Some("C:\\Users\\current")
        );
        // SAFETY: tests serialize env mutation via env_lock.
        unsafe {
            match previous_home {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
            match previous_profile {
                Some(value) => std::env::set_var("USERPROFILE", value),
                None => std::env::remove_var("USERPROFILE"),
            }
        }
    }

    #[test]
    fn dedicated_api_key_never_enters_the_generic_environment_map() {
        let marker = "pillow-test-canary-not-a-real-key";
        let cfg = GrokAcpConfig::new("grok").with_api_key(marker.as_bytes().to_vec());
        let env = build_child_env(&cfg).expect("generic environment");
        assert!(!env.values().any(|value| value.contains(marker)));
        assert!(!env.contains_key(crate::GROK_API_KEY_ENV));
    }
}
