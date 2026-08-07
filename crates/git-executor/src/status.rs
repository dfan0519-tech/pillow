//! Porcelain v2 `-z` status parsing with portable paths and hard bounds.

/// Maximum status entries retained in one snapshot.
pub const STATUS_ENTRY_CAP: usize = 5_000;

/// Maximum aggregate porcelain transport bytes examined for one snapshot.
pub const STATUS_TRANSPORT_CAP: usize = 2 * 1024 * 1024;

/// Stable project-relative repository identity (no native roots).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRepositoryId {
    pub project_relative: String,
}

/// Parsed HEAD identity from porcelain branch headers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitHead {
    Branch { name: String, oid: String },
    Detached { oid: String },
    Unknown,
}

/// Semantic classification of a status entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GitStatusKind {
    Modified,
    Added,
    Deleted,
    Untracked,
    Renamed,
    Copied,
    Typechange,
    Conflict,
    Submodule,
    Ignored,
    Unknown,
}

/// One project-relative status entry. Renames keep old and new paths together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusEntry {
    pub path: String,
    pub old_path: Option<String>,
    pub kind: GitStatusKind,
    /// Two-character porcelain XY codes (ASCII). Untracked uses `??`.
    pub xy: [u8; 2],
}

/// Bounded status snapshot for host/UI consumption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitStatusSnapshot {
    pub repository: GitRepositoryId,
    pub head: GitHead,
    pub entries: Vec<GitStatusEntry>,
    pub truncated: bool,
    pub generation: u64,
}

/// Fail-closed parse errors (no partial success on malformed input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseError {
    Malformed { detail: &'static str },
    PathRejected { detail: &'static str },
    Utf8,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed { detail } => write!(f, "malformed porcelain: {detail}"),
            Self::PathRejected { detail } => write!(f, "path rejected: {detail}"),
            Self::Utf8 => write!(f, "porcelain path is not valid UTF-8"),
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse Git status porcelain v2 output produced with `-z` (NUL record terminators).
///
/// Headers (`# branch.*`) establish [`GitHead`]. Ordinary (`1`), rename/copy (`2`),
/// unmerged (`u`), untracked (`?`) and ignored (`!`) records become entries.
/// Input beyond [`STATUS_TRANSPORT_CAP`] or more than [`STATUS_ENTRY_CAP`] entries
/// sets `truncated` and stops after the last complete in-budget record.
///
/// Trailing partial records and field-shape failures return [`ParseError::Malformed`].
pub fn parse_porcelain_v2_z(
    bytes: &[u8],
    repository: GitRepositoryId,
    generation: u64,
) -> Result<GitStatusSnapshot, ParseError> {
    validate_portable_path(&repository.project_relative)?;

    let mut truncated = false;
    let limit = if bytes.len() > STATUS_TRANSPORT_CAP {
        truncated = true;
        STATUS_TRANSPORT_CAP
    } else {
        bytes.len()
    };
    let window = &bytes[..limit];

    // If we truncated mid-stream, only consume complete NUL-terminated records.
    let parse_end = if truncated {
        match window.iter().rposition(|&b| b == 0) {
            Some(i) => i + 1,
            None => 0,
        }
    } else {
        window.len()
    };
    let stream = &window[..parse_end];

    // Full input with no trailing NUL but non-empty body is a partial record.
    if !truncated && !bytes.is_empty() && bytes.last() != Some(&0) {
        return Err(ParseError::Malformed {
            detail: "trailing partial record without NUL terminator",
        });
    }

    let mut head_oid: Option<String> = None;
    let mut head_name: Option<String> = None;
    let mut detached = false;
    let mut entries: Vec<GitStatusEntry> = Vec::new();
    let mut offset = 0usize;

    while offset < stream.len() {
        if entries.len() >= STATUS_ENTRY_CAP {
            truncated = true;
            break;
        }

        let Some(rel_nul) = stream[offset..].iter().position(|&b| b == 0) else {
            return Err(ParseError::Malformed {
                detail: "missing NUL terminator",
            });
        };
        let record = &stream[offset..offset + rel_nul];
        offset += rel_nul + 1;

        if record.is_empty() {
            return Err(ParseError::Malformed {
                detail: "empty NUL-delimited record",
            });
        }

        match record[0] {
            b'#' => parse_header(record, &mut head_oid, &mut head_name, &mut detached)?,
            b'1' => {
                entries.push(parse_ordinary(record)?);
            }
            b'2' => {
                // Rename/copy: path is inside this record; old path is the next NUL field.
                let (mut entry, needs_old) = parse_rename_head(record)?;
                if needs_old {
                    let Some(rel_nul2) = stream[offset..].iter().position(|&b| b == 0) else {
                        return Err(ParseError::Malformed {
                            detail: "rename missing original path record",
                        });
                    };
                    let old_raw = &stream[offset..offset + rel_nul2];
                    offset += rel_nul2 + 1;
                    let old_path = decode_portable_path(old_raw)?;
                    entry.old_path = Some(old_path);
                } else {
                    return Err(ParseError::Malformed {
                        detail: "rename record missing path",
                    });
                }
                entries.push(entry);
            }
            b'u' => {
                entries.push(parse_unmerged(record)?);
            }
            b'?' => {
                entries.push(parse_simple_path_record(
                    record,
                    b'?',
                    GitStatusKind::Untracked,
                )?);
            }
            b'!' => {
                entries.push(parse_simple_path_record(
                    record,
                    b'!',
                    GitStatusKind::Ignored,
                )?);
            }
            _ => {
                return Err(ParseError::Malformed {
                    detail: "unknown porcelain record type",
                });
            }
        }
    }

    // If we stopped early due to entry cap, more complete records may remain.
    if !truncated && offset < stream.len() {
        // Should not happen unless entry cap hit without setting flag — handled above.
    }
    if entries.len() >= STATUS_ENTRY_CAP {
        // Detect leftover complete records beyond the cap.
        if offset < stream.len() {
            truncated = true;
        }
    }
    // Transport truncation: input longer than cap implies more data may exist.
    if bytes.len() > STATUS_TRANSPORT_CAP {
        truncated = true;
    }

    let head = match (detached, head_name, head_oid) {
        (true, _, Some(oid)) => GitHead::Detached { oid },
        (true, _, None) => GitHead::Detached { oid: String::new() },
        (false, Some(name), Some(oid)) => GitHead::Branch { name, oid },
        (false, Some(name), None) => GitHead::Branch {
            name,
            oid: String::new(),
        },
        _ => GitHead::Unknown,
    };

    Ok(GitStatusSnapshot {
        repository,
        head,
        entries,
        truncated,
        generation,
    })
}

fn parse_header(
    record: &[u8],
    head_oid: &mut Option<String>,
    head_name: &mut Option<String>,
    detached: &mut bool,
) -> Result<(), ParseError> {
    let line = std::str::from_utf8(record).map_err(|_| ParseError::Utf8)?;
    // Format: "# branch.oid <value>" / "# branch.head <value>"
    let rest = line.strip_prefix("# ").unwrap_or(line);
    if let Some(oid) = rest.strip_prefix("branch.oid ") {
        *head_oid = Some(oid.to_string());
        return Ok(());
    }
    if let Some(head) = rest.strip_prefix("branch.head ") {
        if head == "(detached)" {
            *detached = true;
            *head_name = None;
        } else {
            *detached = false;
            *head_name = Some(head.to_string());
        }
        return Ok(());
    }
    // Other headers (upstream, ab) ignored for C4.0 DTO.
    Ok(())
}

fn parse_ordinary(record: &[u8]) -> Result<GitStatusEntry, ParseError> {
    // 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
    let line = std::str::from_utf8(record).map_err(|_| ParseError::Utf8)?;
    let mut parts = line.splitn(9, ' ');
    let tag = parts.next();
    let xy = parts.next();
    let sub = parts.next();
    let _mh = parts.next();
    let _mi = parts.next();
    let _mw = parts.next();
    let _hh = parts.next();
    let _hi = parts.next();
    let path = parts.next();
    if tag != Some("1") {
        return Err(ParseError::Malformed {
            detail: "ordinary tag",
        });
    }
    let (xy_bytes, path, sub) = match (xy, sub, path) {
        (Some(xy), Some(sub), Some(path)) if xy.len() == 2 && !path.is_empty() => {
            ([xy.as_bytes()[0], xy.as_bytes()[1]], path, sub)
        }
        _ => {
            return Err(ParseError::Malformed {
                detail: "ordinary field count",
            });
        }
    };
    validate_portable_path(path)?;
    let kind = if sub.starts_with('S') {
        GitStatusKind::Submodule
    } else {
        kind_from_xy(xy_bytes)
    };
    Ok(GitStatusEntry {
        path: path.to_string(),
        old_path: None,
        kind,
        xy: xy_bytes,
    })
}

fn parse_rename_head(record: &[u8]) -> Result<(GitStatusEntry, bool), ParseError> {
    // 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>
    let line = std::str::from_utf8(record).map_err(|_| ParseError::Utf8)?;
    let mut parts = line.splitn(10, ' ');
    let tag = parts.next();
    let xy = parts.next();
    let sub = parts.next();
    let _mh = parts.next();
    let _mi = parts.next();
    let _mw = parts.next();
    let _hh = parts.next();
    let _hi = parts.next();
    let score = parts.next();
    let path = parts.next();
    if tag != Some("2") {
        return Err(ParseError::Malformed {
            detail: "rename tag",
        });
    }
    let (xy_bytes, path, score) = match (xy, sub, score, path) {
        (Some(xy), Some(_sub), Some(score), Some(path))
            if xy.len() == 2 && !path.is_empty() && !score.is_empty() =>
        {
            ([xy.as_bytes()[0], xy.as_bytes()[1]], path, score)
        }
        _ => {
            return Err(ParseError::Malformed {
                detail: "rename field count",
            });
        }
    };
    validate_portable_path(path)?;
    let kind = match score.as_bytes().first() {
        Some(b'R') => GitStatusKind::Renamed,
        Some(b'C') => GitStatusKind::Copied,
        _ => kind_from_xy(xy_bytes),
    };
    Ok((
        GitStatusEntry {
            path: path.to_string(),
            old_path: None,
            kind,
            xy: xy_bytes,
        },
        true,
    ))
}

fn parse_unmerged(record: &[u8]) -> Result<GitStatusEntry, ParseError> {
    // u <XY> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
    let line = std::str::from_utf8(record).map_err(|_| ParseError::Utf8)?;
    let mut parts = line.splitn(11, ' ');
    let tag = parts.next();
    let xy = parts.next();
    let _sub = parts.next();
    let _m1 = parts.next();
    let _m2 = parts.next();
    let _m3 = parts.next();
    let _mw = parts.next();
    let _h1 = parts.next();
    let _h2 = parts.next();
    let _h3 = parts.next();
    let path = parts.next();
    if tag != Some("u") {
        return Err(ParseError::Malformed {
            detail: "unmerged tag",
        });
    }
    let (xy_bytes, path) = match (xy, path) {
        (Some(xy), Some(path)) if xy.len() == 2 && !path.is_empty() => {
            ([xy.as_bytes()[0], xy.as_bytes()[1]], path)
        }
        _ => {
            return Err(ParseError::Malformed {
                detail: "unmerged field count",
            });
        }
    };
    validate_portable_path(path)?;
    Ok(GitStatusEntry {
        path: path.to_string(),
        old_path: None,
        kind: GitStatusKind::Conflict,
        xy: xy_bytes,
    })
}

fn parse_simple_path_record(
    record: &[u8],
    tag: u8,
    kind: GitStatusKind,
) -> Result<GitStatusEntry, ParseError> {
    // ? <path> / ! <path>
    if record.len() < 3 || record[0] != tag || record[1] != b' ' {
        return Err(ParseError::Malformed {
            detail: "simple path record shape",
        });
    }
    let path = decode_portable_path_or_directory(&record[2..])?;
    let xy = match kind {
        GitStatusKind::Untracked => *b"??",
        GitStatusKind::Ignored => *b"!!",
        _ => [tag, b' '],
    };
    Ok(GitStatusEntry {
        path,
        old_path: None,
        kind,
        xy,
    })
}

fn decode_portable_path(raw: &[u8]) -> Result<String, ParseError> {
    let path = std::str::from_utf8(raw).map_err(|_| ParseError::Utf8)?;
    validate_portable_path(path)?;
    Ok(path.to_string())
}

/// Untracked/ignored records may name a collapsed directory, which porcelain v2
/// reports with exactly one trailing `/` (`? vendor/`). Only these two record
/// kinds can carry that form; tracked, rename and unmerged records never do.
///
/// The stem is validated under the identical portable-path rules, so traversal,
/// absolute, device and non-portable-separator containment is unchanged. The
/// trailing slash is preserved so a directory entry stays distinguishable from
/// a file of the same name and is not mistaken for a diffable path.
fn decode_portable_path_or_directory(raw: &[u8]) -> Result<String, ParseError> {
    let path = std::str::from_utf8(raw).map_err(|_| ParseError::Utf8)?;
    let Some(stem) = path.strip_suffix('/') else {
        validate_portable_path(path)?;
        return Ok(path.to_string());
    };
    // `/`, `vendor//` and `./` are never emitted by git and stay rejected.
    if stem.is_empty() || stem == "." || stem.ends_with('/') {
        return Err(ParseError::PathRejected {
            detail: "empty or repeated trailing separator",
        });
    }
    validate_portable_path(stem)?;
    Ok(path.to_string())
}

fn kind_from_xy(xy: [u8; 2]) -> GitStatusKind {
    if xy[0] == b'U' || xy[1] == b'U' {
        return GitStatusKind::Conflict;
    }
    // Prefer index (first) then worktree (second) for primary kind.
    for code in [xy[0], xy[1]] {
        match code {
            b'R' => return GitStatusKind::Renamed,
            b'C' => return GitStatusKind::Copied,
            b'A' => return GitStatusKind::Added,
            b'D' => return GitStatusKind::Deleted,
            b'T' => return GitStatusKind::Typechange,
            b'M' => return GitStatusKind::Modified,
            _ => {}
        }
    }
    GitStatusKind::Unknown
}

/// Accept only relative portable paths (forward slashes, no traversal/devices).
pub(crate) fn validate_portable_path(path: &str) -> Result<(), ParseError> {
    if path == "." {
        return Ok(());
    }
    if path.is_empty() || path.len() > 4_096 {
        return Err(ParseError::PathRejected {
            detail: "empty or too long",
        });
    }
    if path.starts_with('/') || path.starts_with('\\') || path.contains('\\') || path.contains(':')
    {
        return Err(ParseError::PathRejected {
            detail: "absolute or non-portable separator",
        });
    }
    for component in path.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err(ParseError::PathRejected {
                detail: "empty or traversal component",
            });
        }
        if component.len() > 255 {
            return Err(ParseError::PathRejected {
                detail: "component too long",
            });
        }
        if component.ends_with('.') || component.ends_with(' ') {
            return Err(ParseError::PathRejected {
                detail: "trailing dot or space",
            });
        }
        if component
            .chars()
            .any(|c| c.is_control() || matches!(c, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Err(ParseError::PathRejected {
                detail: "illegal character",
            });
        }
        if is_windows_device_component(component) {
            return Err(ParseError::PathRejected {
                detail: "windows device name",
            });
        }
    }
    Ok(())
}

fn is_windows_device_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .trim_end_matches([' ', '.'])
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    let bytes = stem.as_bytes();
    bytes.len() == 4 && matches!(&bytes[..3], b"COM" | b"LPT") && matches!(bytes[3], b'1'..=b'9')
}
