//! Grok Build ACP adapter for Keeper IDE.
//!
//! Translates NDJSON JSON-RPC 2.0 over stdio into provider-neutral
//! `AgentRuntimeAdapter` events. ACP session ids, raw envelopes, process ids,
//! and credentials never leave this crate.

mod adapter;
mod config;
mod error;
mod map;
mod process;
mod prompt;
mod rpc;
mod state;
mod turn;

pub use adapter::GrokAcpAdapter;
pub use config::GrokAcpConfig;
