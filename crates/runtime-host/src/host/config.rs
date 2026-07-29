//! Host configuration and concurrency-limit normalization.

use std::path::PathBuf;
use std::sync::Arc;

use crate::delivery::DEFAULT_EVENT_QUEUE_CAPACITY;

/// Default concurrent turn limit for the host (process-pool style bound).
pub const DEFAULT_MAX_CONCURRENT_TURNS: u32 = 4;
pub(super) const MAX_CONCURRENT_TURNS_CAP: u32 = 32;

/// Host configuration.
///
/// `after_worker_registered` is a test seam used to force the
/// register-before-handle race with barriers. Production callers leave it
/// `None`.
#[derive(Clone)]
pub struct HostConfig {
    pub max_concurrent_turns: u32,
    /// When true, fixture `delay_ms` is honored; when false, delays are zeroed.
    pub honor_fixture_delays: bool,
    /// Per-turn bounded event queue capacity (backpressure when full).
    pub event_queue_capacity: usize,
    /// Test seam: invoked after the worker is registered (`Starting`) and
    /// before `arm_turn` / thread install. Default `None`.
    #[doc(hidden)]
    pub after_worker_registered: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            max_concurrent_turns: DEFAULT_MAX_CONCURRENT_TURNS,
            honor_fixture_delays: true,
            event_queue_capacity: DEFAULT_EVENT_QUEUE_CAPACITY,
            after_worker_registered: None,
        }
    }
}

impl std::fmt::Debug for HostConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostConfig")
            .field("max_concurrent_turns", &self.max_concurrent_turns)
            .field("honor_fixture_delays", &self.honor_fixture_delays)
            .field("event_queue_capacity", &self.event_queue_capacity)
            .field(
                "after_worker_registered",
                &self.after_worker_registered.is_some(),
            )
            .finish()
    }
}

pub fn normalize_max_concurrent_turns(raw: u32) -> u32 {
    raw.clamp(1, MAX_CONCURRENT_TURNS_CAP)
}

/// Resolve the workspace fixture path for tests that prefer filesystem load.
pub fn workspace_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/protocol/fake-turn.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_concurrent_turns_clamps() {
        assert_eq!(normalize_max_concurrent_turns(0), 1);
        assert_eq!(normalize_max_concurrent_turns(4), 4);
        assert_eq!(normalize_max_concurrent_turns(99), MAX_CONCURRENT_TURNS_CAP);
    }
}
