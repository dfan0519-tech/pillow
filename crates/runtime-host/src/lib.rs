//! Bounded runtime host and deterministic fake adapter for Slice B1.
//!
//! The fake adapter replays a committed JSONL fixture with cooperative
//! cancellation. It never spawns a real agent process, opens a network
//! socket, or touches SQLite.

mod delivery;
mod fake;
mod fixture;
mod governed_read;
mod governed_write;
mod host;
mod sink;
mod worker;

pub use delivery::{BoundedDelivery, DEFAULT_EVENT_QUEUE_CAPACITY};
pub use fake::FakeRuntimeAdapter;
pub use fixture::{FAKE_TURN_FIXTURE, FixtureEventLine, load_fixture_file, parse_fixture_jsonl};
pub use governed_read::{
    GovernedReadBridgeError, GovernedReadDelivery, GovernedReadReceipt, ProviderToolProposal,
    execute_governed_read,
};
#[cfg(any(test, feature = "test-support"))]
pub use governed_write::force_live_expiry_for_tests;
pub use governed_write::{
    APPROVAL_GRANT_RAW_LEN, GovernedWriteBridgeError, GovernedWriteDelivery, GovernedWriteReceipt,
    LiveWriteProposalView, ProviderWriteProposal, WriteDiffView, decide_write_proposal,
    execute_governed_write, generate_approval_grant, grant_digest, is_canonical_grant,
    list_write_proposals, open_write_proposal_diff, recover_write_proposals,
    recover_write_proposals_with_roots,
};
pub use host::{
    DEFAULT_MAX_CONCURRENT_TURNS, GovernedProcessBridgeError, GovernedProcessDelivery,
    GovernedProcessLifecycle, GovernedProcessReceipt, HostConfig, LiveProcessProposalView,
    ProcessOrigin, ProcessOutputChunk, ProcessOutputStream, ProcessOutputView,
    ProcessProposalRequestedView, ProcessRiskClass, ProviderProcessProposal, RuntimeHost,
    SpawnedTurn, decide_process_proposal, execute_governed_process, list_process_proposals,
    live_process_authority_count, normalize_max_concurrent_turns, open_process_output,
    pending_process_run_count, recover_process_proposals, workspace_fixture_path,
};
#[cfg(any(test, feature = "test-support"))]
pub use host::{
    clear_pre_claim_hook_for_tests, force_live_process_expiry_for_tests,
    set_pre_claim_hook_for_tests,
};
pub use sink::{BoundedDeliverySink, CollectingSink, ProtocolGuardSink, TerminalGuardSink};
