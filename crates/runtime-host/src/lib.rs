//! Bounded runtime host and deterministic fake adapter for Slice B1.
//!
//! The fake adapter replays a committed JSONL fixture with cooperative
//! cancellation. It never spawns a real agent process, opens a network
//! socket, or touches SQLite.

mod fake;
mod fixture;
mod host;
mod sink;

pub use fake::FakeRuntimeAdapter;
pub use fixture::{load_fixture_file, parse_fixture_jsonl, FixtureEventLine, FAKE_TURN_FIXTURE};
pub use host::{
    normalize_max_concurrent_turns, workspace_fixture_path, HostConfig, RuntimeHost, SpawnedTurn,
    DEFAULT_MAX_CONCURRENT_TURNS,
};
pub use sink::{CollectingSink, TerminalGuardSink};
