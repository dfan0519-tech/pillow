use std::sync::{Arc, Mutex};

pub const PROCESS_OUTPUT_BYTE_CAP: usize = 262_144;
pub const PROCESS_DISPLAY_LINE_CAP: usize = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputStreamTag {
    Stdout,
    Stderr,
}

/// Renderer-safe retained output. Sequence is assigned in runner observation order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputChunk {
    pub stream: OutputStreamTag,
    pub sequence: u64,
    pub lossy_utf8: bool,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedOutput {
    pub text: String,
    pub bytes_seen: usize,
    pub bytes_retained: usize,
    pub truncated: bool,
    pub lossy_utf8: bool,
}

impl BoundedOutput {
    pub(crate) fn from_parts(bytes: Vec<u8>, bytes_seen: usize, truncated: bool) -> Self {
        let lossy_utf8 = std::str::from_utf8(&bytes).is_err();
        let bytes_retained = bytes.len();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        Self {
            text,
            bytes_seen,
            bytes_retained,
            truncated,
            lossy_utf8,
        }
    }

    pub fn projected_text(&self) -> String {
        neutralize_control_sequences(&self.text)
    }
}

/// Replace C0 controls (apart from TAB/LF/CR) and DEL with spaces.
pub fn neutralize_control_sequences(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\t' | '\n' | '\r' => c,
            c if c.is_control() || c == '\u{7f}' => ' ',
            other => other,
        })
        .collect()
}

pub fn build_output_chunks(stdout: &str, stderr: &str) -> Vec<OutputChunk> {
    let mut chunks = Vec::new();
    if !stdout.is_empty() {
        chunks.push(OutputChunk {
            stream: OutputStreamTag::Stdout,
            sequence: 0,
            lossy_utf8: false,
            text: stdout.to_owned(),
        });
    }
    if !stderr.is_empty() {
        chunks.push(OutputChunk {
            stream: OutputStreamTag::Stderr,
            sequence: chunks.len() as u64,
            lossy_utf8: false,
            text: stderr.to_owned(),
        });
    }
    chunks
}

pub(crate) type SharedCapture = Arc<Mutex<Capture>>;

/// Data-only observer of already-retained display chunks. It never sees raw bytes.
pub type OutputObserver = Arc<dyn Fn(OutputChunk) + Send + Sync>;

pub(crate) struct Capture {
    remaining_bytes: usize,
    pub stdout: StreamCapture,
    pub stderr: StreamCapture,
    observation: Vec<OutputChunk>,
    display_lines_used: usize,
    display_line_open: bool,
    display_truncated: bool,
    observer: Option<OutputObserver>,
}

impl std::fmt::Debug for Capture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Capture")
            .field("remaining_bytes", &self.remaining_bytes)
            .field("observation_len", &self.observation.len())
            .finish()
    }
}

#[derive(Default)]
pub(crate) struct StreamCapture {
    bytes: Vec<u8>,
    bytes_seen: usize,
    truncated: bool,
    lossy: bool,
}

impl Capture {
    #[allow(dead_code)]
    pub(crate) fn new(cap: usize) -> SharedCapture {
        Self::new_with_observer(cap, None)
    }

    pub(crate) fn new_with_observer(cap: usize, observer: Option<OutputObserver>) -> SharedCapture {
        Arc::new(Mutex::new(Self {
            remaining_bytes: cap,
            stdout: StreamCapture::default(),
            stderr: StreamCapture::default(),
            observation: Vec::new(),
            display_lines_used: 0,
            display_line_open: false,
            display_truncated: false,
            observer,
        }))
    }

    pub(crate) fn finish(self) -> (BoundedOutput, BoundedOutput, Vec<OutputChunk>) {
        let mut stdout = BoundedOutput::from_parts(
            self.stdout.bytes,
            self.stdout.bytes_seen,
            self.stdout.truncated || self.display_truncated,
        );
        stdout.lossy_utf8 |= self.stdout.lossy;
        let mut stderr = BoundedOutput::from_parts(
            self.stderr.bytes,
            self.stderr.bytes_seen,
            self.stderr.truncated || self.display_truncated,
        );
        stderr.lossy_utf8 |= self.stderr.lossy;
        (stdout, stderr, self.observation)
    }

    pub(crate) fn append(&mut self, stdout: bool, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let stream = if stdout {
            OutputStreamTag::Stdout
        } else {
            OutputStreamTag::Stderr
        };
        let (fragment, lossy_utf8) = if stdout {
            self.stdout.append(data, &mut self.remaining_bytes)
        } else {
            self.stderr.append(data, &mut self.remaining_bytes)
        };
        let Some(text) = self.retain_display_text(&fragment) else {
            return;
        };
        let chunk = OutputChunk {
            stream,
            sequence: self.observation.len() as u64,
            lossy_utf8,
            text: neutralize_control_sequences(&text),
        };
        if let Some(observer) = &self.observer {
            observer(chunk.clone());
        }
        self.observation.push(chunk);
    }

    fn retain_display_text(&mut self, text: &str) -> Option<String> {
        let mut retained = String::new();
        for ch in text.chars() {
            if !self.display_line_open {
                if self.display_lines_used >= PROCESS_DISPLAY_LINE_CAP {
                    self.display_truncated = true;
                    break;
                }
                self.display_line_open = true;
            }
            retained.push(ch);
            if ch == '\n' {
                self.display_lines_used += 1;
                self.display_line_open = false;
            }
        }
        (!retained.is_empty()).then_some(retained)
    }
}

impl StreamCapture {
    fn append(&mut self, data: &[u8], remaining_bytes: &mut usize) -> (String, bool) {
        self.bytes_seen = self.bytes_seen.saturating_add(data.len());
        let take = data.len().min(*remaining_bytes);
        let slice = &data[..take];
        let lossy = std::str::from_utf8(slice).is_err();
        self.lossy |= lossy;
        self.bytes.extend_from_slice(slice);
        *remaining_bytes -= take;
        self.truncated |= take != data.len();
        (String::from_utf8_lossy(slice).into_owned(), lossy)
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    #[test]
    fn neutralize_strips_escape_and_c0_controls() {
        let projected = neutralize_control_sequences("ok\u{1b}[31mred\u{07}bell\tkeep\n");
        assert!(!projected.contains('\u{1b}') && !projected.contains('\u{07}'));
        assert!(projected.contains('\t') && projected.contains('\n'));
    }
    #[test]
    fn build_chunks_tags_streams_and_sequences() {
        let chunks = build_output_chunks("out", "err");
        assert_eq!(chunks[0].stream, OutputStreamTag::Stdout);
        assert_eq!(chunks[1].stream, OutputStreamTag::Stderr);
        assert_eq!(chunks[1].sequence, 1);
    }
    #[test]
    fn observer_only_receives_retained_display_chunks() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let copy = Arc::clone(&seen);
        let observer: OutputObserver = Arc::new(move |chunk| copy.lock().unwrap().push(chunk));
        let capture = Capture::new_with_observer(4, Some(observer));
        capture.lock().unwrap().append(true, b"ab\x1bcd");
        let capture = Arc::try_unwrap(capture).unwrap().into_inner().unwrap();
        let (_, _, chunks) = capture.finish();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.as_slice(), chunks.as_slice());
        assert!(seen.iter().map(|c| c.text.len()).sum::<usize>() <= 4);
        assert!(!seen.iter().any(|c| c.text.contains('\u{1b}')));
    }
}
