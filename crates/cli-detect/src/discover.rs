//! Filesystem / PATH discovery for agent CLI executables (no process launch).

use std::path::{Path, PathBuf};

use crate::profiles::{ALL_KINDS, AgentCliKind};
use crate::types::Discovery;

/// Discover a single CLI kind using the process environment (PATH + conventional dirs).
pub fn discover(kind: AgentCliKind) -> Discovery {
    discover_in(kind, &path_search_roots())
}

/// Discover all known CLI kinds.
pub fn discover_all() -> Vec<Discovery> {
    ALL_KINDS.iter().copied().map(discover).collect()
}

/// Discover using an explicit ordered list of directories (tests inject fakes here).
pub fn discover_in(kind: AgentCliKind, search_roots: &[PathBuf]) -> Discovery {
    if let Some(path) = find_executable(kind, search_roots) {
        Discovery {
            kind,
            found: true,
            executable: Some(path),
        }
    } else {
        Discovery {
            kind,
            found: false,
            executable: None,
        }
    }
}

fn find_executable(kind: AgentCliKind, search_roots: &[PathBuf]) -> Option<PathBuf> {
    for root in search_roots {
        for name in kind.binary_names() {
            for candidate in candidate_names(name) {
                let path = root.join(&candidate);
                if is_runnable_file(&path) {
                    return Some(normalize_path(path));
                }
            }
        }
        // npm global layout: prefer native vendor binaries when present.
        if let Some(path) = find_npm_vendor_binary(kind, root) {
            return Some(path);
        }
    }
    None
}

fn candidate_names(base: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        // Prefer native PE images; accept PATHEXT-style extensions next.
        vec![
            format!("{base}.exe"),
            base.to_string(),
            format!("{base}.cmd"),
            format!("{base}.bat"),
        ]
    }
    #[cfg(not(windows))]
    {
        vec![base.to_string()]
    }
}

fn is_runnable_file(path: &Path) -> bool {
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file(),
        Err(_) => false,
    }
}

fn normalize_path(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// Build default search roots: PATH dirs, then conventional install locations.
pub(crate) fn path_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            if !dir.as_os_str().is_empty() {
                roots.push(dir);
            }
        }
    }
    for extra in conventional_install_dirs() {
        if !roots.iter().any(|r| r == &extra) {
            roots.push(extra);
        }
    }
    roots
}

fn conventional_install_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    // User-local bin (Unix tools on Windows, cargo, etc.).
    if let Some(home) = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")) {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local").join("bin"));
        dirs.push(home.join(".grok").join("bin"));
        dirs.push(home.join(".cargo").join("bin"));
    }
    #[cfg(windows)]
    {
        if let Some(local) = std::env::var_os("LOCALAPPDATA") {
            let local = PathBuf::from(local);
            dirs.push(local.join("Programs"));
        }
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let appdata = PathBuf::from(appdata);
            dirs.push(appdata.join("npm"));
            // npm package install roots (native vendor bins live under here).
            dirs.push(appdata.join("npm").join("node_modules"));
        }
    }
    #[cfg(not(windows))]
    {
        dirs.push(PathBuf::from("/usr/local/bin"));
        dirs.push(PathBuf::from("/usr/bin"));
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            dirs.push(home.join(".npm-global").join("bin"));
        }
    }
    dirs
}

/// Known npm package layouts that ship a native `.exe` we can spawn without a shell.
fn find_npm_vendor_binary(kind: AgentCliKind, root: &Path) -> Option<PathBuf> {
    // `root` may be `…/npm` or `…/npm/node_modules`.
    let module_roots = [
        root.to_path_buf(),
        root.join("node_modules"),
        root.join("@openai").join("codex").join("node_modules"),
    ];
    match kind {
        AgentCliKind::Codex => {
            // @openai/codex → platform vendor package → bin/codex.exe
            let relative = [
                PathBuf::from("@openai")
                    .join("codex")
                    .join("node_modules")
                    .join("@openai")
                    .join("codex-win32-x64")
                    .join("vendor")
                    .join("x86_64-pc-windows-msvc")
                    .join("bin")
                    .join("codex.exe"),
                PathBuf::from("@openai")
                    .join("codex-win32-x64")
                    .join("vendor")
                    .join("x86_64-pc-windows-msvc")
                    .join("bin")
                    .join("codex.exe"),
            ];
            for module_root in &module_roots {
                for rel in &relative {
                    let path = module_root.join(rel);
                    if is_runnable_file(&path) {
                        return Some(normalize_path(path));
                    }
                }
            }
            None
        }
        AgentCliKind::Gemini => {
            // Prefer the JS entry + later resolve via node in the probe layer.
            // Discovery itself only returns a path that exists as a file.
            let candidates = [
                PathBuf::from("@google")
                    .join("gemini-cli")
                    .join("bundle")
                    .join("gemini.js"),
                PathBuf::from("gemini-cli").join("bundle").join("gemini.js"),
            ];
            for module_root in &module_roots {
                for rel in &candidates {
                    let path = module_root.join(rel);
                    if is_runnable_file(&path) {
                        return Some(normalize_path(path));
                    }
                }
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    #[test]
    fn discover_in_finds_named_binary_in_search_root() {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let name = "grok.exe";
        #[cfg(not(windows))]
        let name = "grok";
        let path = dir.path().join(name);
        let mut f = fs::File::create(&path).unwrap();
        writeln!(f, "fake").unwrap();
        drop(f);

        let d = discover_in(AgentCliKind::Grok, &[dir.path().to_path_buf()]);
        assert!(d.found);
        assert!(d.executable.is_some());
    }

    #[test]
    fn discover_in_reports_not_found_when_absent() {
        let dir = tempfile::tempdir().unwrap();
        let d = discover_in(AgentCliKind::Claude, &[dir.path().to_path_buf()]);
        assert!(!d.found);
        assert!(d.executable.is_none());
    }
}
