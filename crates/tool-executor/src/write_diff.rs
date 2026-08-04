//! Host-owned bounded unified diff construction.
//!
//! Deterministic, linear-time relative to input size: common prefix/suffix
//! matching only (no unbounded quadratic LCS). Output is capped at
//! [`DIFF_BYTE_CAP`] bytes and [`DIFF_LINE_CAP`] lines.
//!
//! Diffs are zero-context: header line ranges describe only the emitted
//! deletion/insertion lines (no context lines unless a future revision
//! adjusts both header counts and body together).

/// Maximum bytes of unified-diff *output* retained for review display.
pub const DIFF_BYTE_CAP: u64 = 262_144;

/// Maximum number of output lines retained for review display.
pub const DIFF_LINE_CAP: u32 = 4_000;

const TRUNCATION_MARKER: &str = "\n... [diff truncated] ...\n";
const NO_NEWLINE_MARKER: &str = "\\ No newline at end of file\n";

/// Bounded unified-diff snapshot for UI review.
///
/// Debug redacts the unified source text so distinctive content never appears
/// in `format!("{value:?}")`. No `Serialize` impl.
#[derive(Clone, PartialEq, Eq)]
pub struct BoundedDiff {
    pub unified: String,
    pub truncated: bool,
    pub line_count: u32,
    pub byte_count: u64,
}

impl std::fmt::Debug for BoundedDiff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundedDiff")
            .field("unified", &"<redacted>")
            .field("truncated", &self.truncated)
            .field("line_count", &self.line_count)
            .field("byte_count", &self.byte_count)
            .finish()
    }
}

/// Build a deterministic, bounded unified diff from optional base and proposed text.
///
/// `base == None` means create (compare against `/dev/null`). Content itself is
/// never persisted by this function.
pub fn build_bounded_unified_diff(base: Option<&str>, proposed: &str) -> BoundedDiff {
    let base_text = base.unwrap_or("");
    let base_lines = split_lines(base_text);
    let proposed_lines = split_lines(proposed);

    let mut out = String::new();
    let mut truncated = false;

    if base.is_none() {
        out.push_str("--- /dev/null\n");
    } else {
        out.push_str("--- a\n");
    }
    out.push_str("+++ b\n");

    if base.is_some()
        && base_lines.lines == proposed_lines.lines
        && base_lines.ends_with_newline == proposed_lines.ends_with_newline
    {
        // Identical: file headers only.
        return finalize(out, false);
    }

    // Final-newline-only change: line bodies match but EOF newline flag differs.
    // Must emit the affected last line plus the standard marker (never an empty hunk).
    if base.is_some()
        && base_lines.lines == proposed_lines.lines
        && !base_lines.lines.is_empty()
        && base_lines.ends_with_newline != proposed_lines.ends_with_newline
    {
        let n = base_lines.lines.len();
        let last = &base_lines.lines[n - 1];
        let header = format!("@@ -{n},1 +{n},1 @@\n");
        if !append_line(&mut out, &header, &mut truncated) {
            return finalize(out, true);
        }
        let remove = format!("-{last}\n");
        if !append_line(&mut out, &remove, &mut truncated) {
            return finalize(out, true);
        }
        if !base_lines.ends_with_newline
            && !append_line(&mut out, NO_NEWLINE_MARKER, &mut truncated)
        {
            return finalize(out, true);
        }
        let add = format!("+{last}\n");
        if !append_line(&mut out, &add, &mut truncated) {
            return finalize(out, true);
        }
        if !proposed_lines.ends_with_newline
            && !append_line(&mut out, NO_NEWLINE_MARKER, &mut truncated)
        {
            return finalize(out, true);
        }
        return finalize(out, truncated);
    }

    // Common prefix / suffix (linear in input line count).
    let min_len = base_lines.lines.len().min(proposed_lines.lines.len());
    let mut prefix = 0usize;
    while prefix < min_len && base_lines.lines[prefix] == proposed_lines.lines[prefix] {
        prefix += 1;
    }

    let mut suffix = 0usize;
    let max_suffix = base_lines
        .lines
        .len()
        .saturating_sub(prefix)
        .min(proposed_lines.lines.len().saturating_sub(prefix));
    while suffix < max_suffix
        && base_lines.lines[base_lines.lines.len() - 1 - suffix]
            == proposed_lines.lines[proposed_lines.lines.len() - 1 - suffix]
    {
        suffix += 1;
    }

    let old_count = base_lines.lines.len() - prefix - suffix;
    let new_count = proposed_lines.lines.len() - prefix - suffix;

    // Unified diff empty-side convention:
    // - count > 0: start is 1-based index of first changed line
    // - count == 0: start is the number of retained lines before the edit point
    //   (0 when the side is empty / edit is at the beginning)
    let old_start = if old_count == 0 { prefix } else { prefix + 1 };
    let new_start = if new_count == 0 { prefix } else { prefix + 1 };

    // Zero-context: header ranges match emitted body lines exactly (no context rows).
    let header = format!("@@ -{old_start},{old_count} +{new_start},{new_count} @@\n");
    if !append_line(&mut out, &header, &mut truncated) {
        return finalize(out, true);
    }

    let old_end = base_lines.lines.len() - suffix;
    for line in &base_lines.lines[prefix..old_end] {
        let row = format!("-{line}\n");
        if !append_line(&mut out, &row, &mut truncated) {
            return finalize(out, true);
        }
    }
    // Base missing final newline when the removed region includes the file's last line.
    if !base_lines.ends_with_newline
        && !base_lines.lines.is_empty()
        && suffix == 0
        && old_count > 0
        && !append_line(&mut out, NO_NEWLINE_MARKER, &mut truncated)
    {
        return finalize(out, true);
    }

    let new_end = proposed_lines.lines.len() - suffix;
    for line in &proposed_lines.lines[prefix..new_end] {
        let row = format!("+{line}\n");
        if !append_line(&mut out, &row, &mut truncated) {
            return finalize(out, true);
        }
    }
    if !proposed_lines.ends_with_newline
        && !proposed_lines.lines.is_empty()
        && suffix == 0
        && new_count > 0
        && !append_line(&mut out, NO_NEWLINE_MARKER, &mut truncated)
    {
        return finalize(out, true);
    }

    finalize(out, truncated)
}

struct LineSplit {
    lines: Vec<String>,
    ends_with_newline: bool,
}

fn split_lines(text: &str) -> LineSplit {
    if text.is_empty() {
        return LineSplit {
            lines: Vec::new(),
            ends_with_newline: false,
        };
    }
    let ends_with_newline = text.ends_with('\n');
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    // split always yields a trailing empty after final \n — drop it.
    if ends_with_newline && lines.last().is_some_and(|s| s.is_empty()) {
        lines.pop();
    }
    LineSplit {
        lines,
        ends_with_newline,
    }
}

/// Append `chunk` if it fits under both caps. Returns false when truncation was applied.
fn append_line(out: &mut String, chunk: &str, truncated: &mut bool) -> bool {
    if *truncated {
        return false;
    }
    let next_lines = count_lines(out) + count_lines(chunk);
    let next_bytes = out.len() as u64 + chunk.len() as u64;
    if next_lines > DIFF_LINE_CAP || next_bytes > DIFF_BYTE_CAP {
        apply_truncation_marker(out);
        *truncated = true;
        return false;
    }
    out.push_str(chunk);
    true
}

/// Ensure a truncation marker is present while staying under both caps.
fn apply_truncation_marker(out: &mut String) {
    let marker = TRUNCATION_MARKER;
    let marker_lines = count_lines(marker);
    let marker_bytes = marker.len() as u64;

    // Drop trailing lines until the marker fits under both caps.
    while count_lines(out) + marker_lines > DIFF_LINE_CAP
        || (out.len() as u64) + marker_bytes > DIFF_BYTE_CAP
    {
        if let Some(pos) = out.rfind('\n') {
            if pos == 0 {
                out.clear();
                break;
            }
            // Keep the newline that ends the previous line when possible.
            if pos + 1 == out.len() {
                // out ends with newline — drop that entire last line.
                if let Some(prev) = out[..pos].rfind('\n') {
                    out.truncate(prev + 1);
                } else {
                    out.clear();
                    break;
                }
            } else {
                out.truncate(pos + 1);
            }
        } else {
            out.clear();
            break;
        }
    }

    if count_lines(out) + marker_lines <= DIFF_LINE_CAP
        && (out.len() as u64) + marker_bytes <= DIFF_BYTE_CAP
    {
        out.push_str(marker);
    } else {
        // Extremely tight budget: write as much of the marker as possible.
        let room_bytes = DIFF_BYTE_CAP.saturating_sub(out.len() as u64) as usize;
        if room_bytes > 0 {
            out.push_str(&marker[..room_bytes.min(marker.len())]);
        }
    }
    trim_to_caps(out);
}

fn count_lines(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    // Count newline terminators; text without trailing newline still has one logical line.
    let newlines = text.bytes().filter(|b| *b == b'\n').count() as u32;
    if text.ends_with('\n') {
        newlines
    } else {
        newlines + 1
    }
}

fn trim_to_caps(out: &mut String) {
    while count_lines(out) > DIFF_LINE_CAP || out.len() as u64 > DIFF_BYTE_CAP {
        if let Some(pos) = out.rfind('\n') {
            if pos + 1 < out.len() {
                out.truncate(pos + 1);
            } else if pos > 0 {
                out.truncate(pos);
            } else {
                out.clear();
                break;
            }
        } else {
            let max = (DIFF_BYTE_CAP as usize).min(out.len());
            out.truncate(max);
            break;
        }
        if out.len() as u64 > DIFF_BYTE_CAP {
            out.truncate(DIFF_BYTE_CAP as usize);
        }
    }
}

fn finalize(out: String, truncated: bool) -> BoundedDiff {
    let mut out = out;
    let mut truncated = truncated;
    if count_lines(&out) > DIFF_LINE_CAP || out.len() as u64 > DIFF_BYTE_CAP {
        trim_to_caps(&mut out);
        truncated = true;
    }
    let line_count = count_lines(&out);
    let byte_count = out.len() as u64;
    BoundedDiff {
        unified: out,
        truncated,
        line_count,
        byte_count,
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn split_handles_final_newline() {
        let a = split_lines("a\nb\n");
        assert_eq!(a.lines, vec!["a", "b"]);
        assert!(a.ends_with_newline);
        let b = split_lines("a\nb");
        assert_eq!(b.lines, vec!["a", "b"]);
        assert!(!b.ends_with_newline);
    }
}
