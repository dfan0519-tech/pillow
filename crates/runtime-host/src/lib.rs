//! Bounded runtime host and deterministic fake adapter for Slice B1.
//!
//! The fake adapter replays a committed JSONL fixture with cooperative
//! cancellation. It never spawns a real agent process, opens a network
//! socket, or touches SQLite.

mod delivery;
mod fake;
mod fixture;
mod host;
mod sink;

pub use delivery::{BoundedDelivery, DEFAULT_EVENT_QUEUE_CAPACITY};
pub use fake::FakeRuntimeAdapter;
pub use fixture::{FAKE_TURN_FIXTURE, FixtureEventLine, load_fixture_file, parse_fixture_jsonl};
pub use host::{
    DEFAULT_MAX_CONCURRENT_TURNS, HostConfig, RuntimeHost, SpawnedTurn,
    normalize_max_concurrent_turns, workspace_fixture_path,
};
pub use sink::{BoundedDeliverySink, CollectingSink, ProtocolGuardSink, TerminalGuardSink};
