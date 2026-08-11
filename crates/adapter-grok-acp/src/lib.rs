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

/// Re-export for process-tree cleanup tests (PID liveness without secrets).
#[doc(hidden)]
pub fn process_gone_for_tests(pid: u32) -> bool {
    process::process_gone(pid).unwrap_or(false)
}
