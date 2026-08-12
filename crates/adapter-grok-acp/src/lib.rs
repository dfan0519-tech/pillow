//! Grok Build ACP adapter for Pillow.
//!
//! Translates NDJSON JSON-RPC 2.0 over stdio into provider-neutral
//! `AgentRuntimeAdapter` events. ACP session ids, raw envelopes, process ids,
//! and credentials never leave this crate.

mod adapter;
mod config;
mod error;
mod health;
mod map;
mod process;
mod prompt;
mod provider_processes;
mod provider_terminals;
mod provider_tools;
mod provider_writes;
mod rpc;
mod state;
mod turn;

pub use adapter::GrokAcpAdapter;
pub use config::{GROK_API_KEY_ENV, GrokAcpConfig, PRODUCTION_ACP_ARGV};

/// Pins a process identity for process-tree cleanup tests.
///
/// Replaces an earlier `process_gone_for_tests(pid) -> bool` helper. That one
/// asked about a pid, which names a process only until it exits and the last
/// handle to it closes; after a teardown the number may already belong to
/// something else. It also collapsed a failed OS query to `false`, which reads
/// as "still running" — a passing setup assertion and a failing teardown
/// assertion, so an unanswerable query silently became whichever verdict was
/// wrong. Capturing a probe before the teardown answers about the process the
/// caller means, and reports a failed query as an error.
#[doc(hidden)]
pub use keeper_process_executor::ProcessLivenessProbe;
