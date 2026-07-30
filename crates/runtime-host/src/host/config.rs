//! Host configuration and concurrency-limit normalization.

use std::path::PathBuf;
#[cfg(any(test, feature = "test-support"))]
use std::sync::Arc;

use crate::delivery::DEFAULT_EVENT_QUEUE_CAPACITY;

/// Default concurrent turn limit for the host (process-pool style bound).
pub const DEFAULT_MAX_CONCURRENT_TURNS: u32 = 4;
pub(super) const MAX_CONCURRENT_TURNS_CAP: u32 = 32;

/// Host configuration.
///
/// Prefer [`HostConfig::new`] (or [`Default`]) for construction. The optional
/// post-registration test seam is compiled only under `cfg(test)` or the
/// non-default `test-support` feature and is installed via
/// [`HostConfig::with_after_worker_registered`].
#[derive(Clone)]
pub struct HostConfig {
    pub max_concurrent_turns: u32,
    /// When true, fixture `delay_ms` is honored; when false, delays are zeroed.
    pub honor_fixture_delays: bool,
    /// Per-turn bounded event queue capacity (backpressure when full).
    pub event_queue_capacity: usize,
    /// Test seam: invoked after the worker is registered (`Starting`) and
    /// before `arm_turn` / thread install. Omitted from release builds.
    /// Crate-private: not constructible from outside via field assignment;
    /// install only through [`HostConfig::with_after_worker_registered`].
    #[cfg(any(test, feature = "test-support"))]
    pub(crate) after_worker_registered: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for HostConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_CONCURRENT_TURNS,
            true,
            DEFAULT_EVENT_QUEUE_CAPACITY,
        )
    }
}

impl std::fmt::Debug for HostConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("HostConfig");
        debug
            .field("max_concurrent_turns", &self.max_concurrent_turns)
            .field("honor_fixture_delays", &self.honor_fixture_delays)
            .field("event_queue_capacity", &self.event_queue_capacity);
        #[cfg(any(test, feature = "test-support"))]
        debug.field(
            "after_worker_registered",
            &self.after_worker_registered.is_some(),
        );
        debug.finish()
    }
}

impl HostConfig {
    /// Construct host configuration without test seams.
    pub fn new(
        max_concurrent_turns: u32,
        honor_fixture_delays: bool,
        event_queue_capacity: usize,
    ) -> Self {
        Self {
            max_concurrent_turns,
            honor_fixture_delays,
            event_queue_capacity,
            #[cfg(any(test, feature = "test-support"))]
            after_worker_registered: None,
        }
    }

    /// Install a post-worker-registration barrier used by race tests.
    ///
    /// Available when building this crate's unit tests (`cfg(test)`) or when
    /// the non-default `test-support` feature is enabled (desktop / external
    /// test crates). Release production builds do not compile this method.
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_after_worker_registered(mut self, hook: Arc<dyn Fn() + Send + Sync>) -> Self {
        self.after_worker_registered = Some(hook);
        self
    }

    /// Borrow the test registration hook when the seam is compiled in.
    #[cfg(any(test, feature = "test-support"))]
    pub(super) fn after_worker_registered_hook(&self) -> Option<&Arc<dyn Fn() + Send + Sync>> {
        self.after_worker_registered.as_ref()
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
