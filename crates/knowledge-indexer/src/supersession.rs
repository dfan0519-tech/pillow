//! Explicit supersession extraction only.
//!
//! **查证 (verified):** stored as DB links / status.
//! **推断 (inferred):** returned separately for the audit report — never written.

/// One explicit supersession edge declared by a source document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplicitSupersession {
    /// Path of the declaring document (project-relative).
    pub from_path: String,
    /// Declared target as written (portable path or free-text claim).
    pub to_path: String,
    /// True when `to_path` looks like a relative file path (not free text).
    pub target_is_path_shaped: bool,
}

/// Soft / inferred supersession — report only, never DB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InferredSupersession {
    pub from_path: String,
    pub to_path: Option<String>,
    pub reason: String,
}

/// Full extract for one document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SupersessionExtract {
    pub explicit: Vec<ExplicitSupersession>,
    pub inferred: Vec<InferredSupersession>,
}

/// Extract supersession claims from document content.
///
/// Only **explicit first-person declarations** create [`ExplicitSupersession`]
/// rows suitable for the database (e.g. `> 本文件取代 …`, `This report
/// supersedes …`). Tables that *quote* other documents' claims, soft pointers
/// (“current state moved to …”), and product-language mentions are either
/// ignored or go to `inferred` — never the DB as verified edges.
pub fn extract_supersession(from_path: &str, content: &str) -> SupersessionExtract {
    let mut out = SupersessionExtract::default();

    extract_self_declaration_edges(from_path, content, &mut out);
    extract_soft_moved_to(from_path, content, &mut out);

    out
}

// ---------------------------------------------------------------------------
// Explicit patterns
// ---------------------------------------------------------------------------

/// First-person Chinese / English supersession declarations only.
fn extract_self_declaration_edges(from_path: &str, content: &str, out: &mut SupersessionExtract) {
    let lines: Vec<&str> = content.lines().collect();
    for i in 0..lines.len() {
        if !is_self_declaration_line(lines[i]) {
            continue;
        }
        // Declaration line + up to two continuation lines (1→N wrap).
        let mut window = lines[i].to_owned();
        if i + 1 < lines.len() {
            window.push(' ');
            window.push_str(lines[i + 1]);
        }
        if i + 2 < lines.len() {
            window.push(' ');
            window.push_str(lines[i + 2]);
        }

        let mut found_path = false;
        for target in extract_document_path_candidates(&window) {
            found_path = true;
            push_explicit(from_path, target, out);
        }

        // Pathless free-text claim on the declaration line itself.
        let line_lower = lines[i].to_ascii_lowercase();
        if !found_path
            && (line_lower.contains("supersedes prior delivery status")
                || line_lower.contains("supersedes prior delivery"))
        {
            let claim = "prior delivery status".to_owned();
            if !out
                .explicit
                .iter()
                .any(|e| e.from_path == from_path && e.to_path == claim)
            {
                out.explicit.push(ExplicitSupersession {
                    from_path: from_path.to_owned(),
                    to_path: claim,
                    target_is_path_shaped: false,
                });
            }
        }
    }
}

/// True only for first-person self-claims, not table rows documenting others.
fn is_self_declaration_line(line: &str) -> bool {
    let trimmed = line.trim();
    // Table rows quoting other docs: `| `path` | 本文件取代 … |`
    if trimmed.starts_with('|') {
        return false;
    }
    // Strip blockquote / list markers.
    let body = trimmed
        .trim_start_matches('>')
        .trim_start()
        .trim_start_matches(['-', '*'])
        .trim_start();
    if body.starts_with("本文件取代") || body.starts_with("本文件替换") {
        return true;
    }
    let lower = body.to_ascii_lowercase();
    lower.starts_with("this file supersedes")
        || lower.starts_with("this document supersedes")
        || lower.starts_with("this report supersedes")
        || lower.starts_with("this handoff supersedes")
}

/// Soft pointer: “current state moved to …” → inferred only.
/// Skips markdown table rows that *document* other files' soft pointers.
fn extract_soft_moved_to(from_path: &str, content: &str, out: &mut SupersessionExtract) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') {
            // Inventory / research tables quoting third-party soft pointers.
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower.contains("current state moved to")
            || lower.contains("current status moved to")
            || content_line_has_chinese_soft_pointer(line)
        {
            let targets = extract_backtick_paths(line);
            let targets = if targets.is_empty() {
                extract_docs_path_tokens(line)
            } else {
                targets
            };
            if targets.is_empty() {
                out.inferred.push(InferredSupersession {
                    from_path: from_path.to_owned(),
                    to_path: None,
                    reason: format!("soft pointer without path: {}", truncate(line, 120)),
                });
            } else {
                for t in targets {
                    out.inferred.push(InferredSupersession {
                        from_path: from_path.to_owned(),
                        to_path: Some(t),
                        reason: "soft 'current state moved to' pointer (not explicit 本文件取代)"
                            .into(),
                    });
                }
            }
        }
    }
}

fn content_line_has_chinese_soft_pointer(line: &str) -> bool {
    line.contains("当前状态见") || line.contains("现状已移至") || line.contains("状态已迁至")
}

fn push_explicit(from_path: &str, target: String, out: &mut SupersessionExtract) {
    let target = normalize_declared_path(&target);
    if target.is_empty() || target == from_path {
        return;
    }
    // Drop URLs, absolute paths, and other non-document tokens that can appear
    // in the same multi-line window as a real 本文件取代 claim.
    if is_rejected_supersession_target(&target) {
        return;
    }
    if out
        .explicit
        .iter()
        .any(|e| e.from_path == from_path && e.to_path == target)
    {
        return;
    }
    let target_is_path_shaped = looks_path_shaped(&target);
    out.explicit.push(ExplicitSupersession {
        from_path: from_path.to_owned(),
        to_path: target,
        target_is_path_shaped,
    });
}

fn is_rejected_supersession_target(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("git@")
        || lower.contains("://")
    {
        return true;
    }
    // Absolute / drive-letter paths (never portable registry paths).
    if value.starts_with('/')
        || value.starts_with('\\')
        || value.contains('\\')
        || (value.len() >= 2 && value.as_bytes()[1] == b':')
    {
        return true;
    }
    // Incomplete directory prefixes (`docs/`) or empty components.
    if value.ends_with('/') || value.contains("//") || value == "docs" {
        return true;
    }
    // Path-shaped claims must be markdown documents (or multi-word free text).
    if looks_path_shaped(value) && !value.ends_with(".md") && !value.contains(' ') {
        return true;
    }
    // Bare commit hashes or other non-path backtick noise near supersession lines.
    if !looks_path_shaped(value) && !value.contains(' ') {
        if value.chars().all(|c| c.is_ascii_hexdigit()) && value.len() >= 7 {
            return true;
        }
        if value == "main" || value == "master" || value == "HEAD" {
            return true;
        }
    }
    false
}

fn looks_path_shaped(value: &str) -> bool {
    value.contains('/') || value.ends_with(".md")
}

/// Document-path candidates from a declaration window (backticks + bare docs/*.md).
fn extract_document_path_candidates(text: &str) -> Vec<String> {
    let mut out = extract_backtick_paths(text);
    for p in extract_docs_path_tokens(text) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

fn normalize_declared_path(raw: &str) -> String {
    let t = raw
        .trim()
        .trim_matches(|c| c == '`' || c == '"' || c == '\'' || c == '。' || c == '.');
    let t = t.trim();
    // Strip trailing Chinese punctuation.
    t.trim_end_matches(['。', '，', ',', ';', '；'])
        .replace('\\', "/")
}

fn extract_backtick_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('`') {
            let inner = after[..end].trim();
            // Prefer document-path shaped backticks; skip URLs and bare tokens.
            if !inner.is_empty()
                && (inner.ends_with(".md")
                    || inner.starts_with("docs/")
                    || (inner.contains('/') && !inner.contains("://")))
                && !is_rejected_supersession_target(inner)
            {
                out.push(normalize_declared_path(inner));
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    out
}

fn extract_docs_path_tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // Walk by char boundary, not raw bytes (Chinese prose is common in this corpus).
    let mut i = 0;
    while i < text.len() {
        if text[i..].starts_with("docs/") {
            let start = i;
            let mut end = i + "docs/".len();
            while end < text.len() {
                let ch = text[end..].chars().next().unwrap();
                if ch.is_ascii_alphanumeric() || ch == '/' || ch == '-' || ch == '_' || ch == '.' {
                    end += ch.len_utf8();
                } else {
                    break;
                }
            }
            let token = normalize_declared_path(&text[start..end]);
            if token.ends_with(".md") {
                out.push(token);
            }
            i = end;
        } else {
            let ch = text[i..].chars().next().unwrap();
            i += ch.len_utf8();
        }
    }
    out
}

fn truncate(s: &str, max: usize) -> String {
    let t: String = s.chars().take(max).collect();
    if s.chars().count() > max {
        format!("{t}…")
    } else {
        t
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn extracts_one_to_two_chinese_handoff() {
        let content = r#"> 本文件取代 `docs/handoff/2026-08-07-pillow-controller-handoff.md` 与
> `docs/handoff/2026-08-08-round5-tech-debt-status.md`。旧文件描述的是 C4 验收期的状态,
"#;
        let ext = extract_supersession(
            "docs/handoff/2026-08-08-pillow-controller-handoff.md",
            content,
        );
        assert_eq!(ext.explicit.len(), 2);
        let targets: Vec<_> = ext.explicit.iter().map(|e| e.to_path.as_str()).collect();
        assert!(targets.contains(&"docs/handoff/2026-08-07-pillow-controller-handoff.md"));
        assert!(targets.contains(&"docs/handoff/2026-08-08-round5-tech-debt-status.md"));
        assert!(ext.inferred.is_empty());
    }

    #[test]
    fn pathless_prior_delivery_is_explicit_free_text() {
        let content =
            "This report supersedes prior delivery status: controller **final-fix** findings\n";
        let ext = extract_supersession(
            "docs/execution/2026-08-01-wave-c3e-approval-ui-report.md",
            content,
        );
        assert_eq!(ext.explicit.len(), 1);
        assert_eq!(ext.explicit[0].to_path, "prior delivery status");
        assert!(!ext.explicit[0].target_is_path_shaped);
    }

    #[test]
    fn soft_moved_to_is_inferred_not_explicit() {
        let content = "current state moved to `docs/roadmap/2026-07-31-stage1-overall-status.md`\n";
        let ext =
            extract_supersession("docs/roadmap/2026-07-30-stage1-plan-comparison.md", content);
        assert!(ext.explicit.is_empty(), "inferred must not enter explicit");
        assert_eq!(ext.inferred.len(), 1);
        assert_eq!(
            ext.inferred[0].to_path.as_deref(),
            Some("docs/roadmap/2026-07-31-stage1-overall-status.md")
        );
    }
}
