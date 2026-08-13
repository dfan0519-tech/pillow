//! Bounded NDJSON JSON-RPC client.

mod client;
mod framer;
mod methods;

pub use client::{Incoming, READER_EXIT_GRACE, ReaderExit, RpcClient};
pub use methods::{
    initialize_params, permission_result, session_cancel_params, session_load_params,
    session_new_params, session_prompt_params,
};
