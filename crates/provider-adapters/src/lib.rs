//! Pure provider-adapter descriptions and request preparation.
//!
//! This crate answers “what request should be sent?” for a given provider
//! shape, credential, model, and body. It never opens sockets, never starts
//! processes, and never reads credential stores.
//!
//! # Boundary
//!
//! - **In scope:** adapter descriptors, credential *shape* (not storage),
//!   pure `prepare_request`, model allowlists, derived-header resolution,
//!   shell-expressibility classification for runtime path selection.
//! - **Out of scope:** HTTP, OAuth login, DPAPI / secret files, UI, runtime
//!   slot wiring (consumers live in `pillow-runtime-slots`).

mod adapters;
mod credential;
mod jwt;
mod prepare;
mod redact;
mod shell;
mod types;

pub use adapters::{
    CHATGPT_CODEX_ENDPOINT, CHATGPT_CODEX_MODEL_ALLOWLIST, chatgpt_codex_subscription,
    openai_compatible,
};
pub use credential::{Credential, CredentialKind, SecretToken};
pub use jwt::{extract_chatgpt_account_id, parse_jwt_payload_json};
pub use prepare::{PrepareError, prepare_request};
pub use shell::{AdapterLookupError, is_grok_shell_expressible, request_adapter_for_provider};
pub use types::{
    AuthInjection, DerivedHeader, PreparedRequest, ProtocolShape, ProviderAdapter, RequestHeaders,
};
