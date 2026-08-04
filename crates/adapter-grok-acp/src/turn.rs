//! Arm / cancel / active prompt state for concurrent host spawn.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use keeper_agent_protocol::{RunId, RuntimeError};

use crate::map::ToolRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderRequestIdentity {
    canonical_request: String,
    call_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderRequestIdentityError {
    Conflict,
    Exhausted,
}

/// Native ACP request ids are transport-local correlation keys. This registry
/// maps them to Pillow-owned durable identities for one active run.
#[derive(Debug, Default)]
pub(crate) struct ProviderRequestIdentityRegistry {
    next_ordinal: u64,
    requests: HashMap<String, ProviderRequestIdentity>,
}

impl ProviderRequestIdentityRegistry {
    pub(crate) fn resolve(
        &mut self,
        run_id: RunId,
        transport_id: String,
        canonical_request: String,
    ) -> Result<String, ProviderRequestIdentityError> {
        if let Some(existing) = self.requests.get(&transport_id) {
            return if existing.canonical_request == canonical_request {
                Ok(existing.call_id.clone())
            } else {
                Err(ProviderRequestIdentityError::Conflict)
            };
        }

        let ordinal = self
            .next_ordinal
            .checked_add(1)
            .ok_or(ProviderRequestIdentityError::Exhausted)?;
        let call_id = format!("grok-acp:{run_id}:{ordinal}");
        self.next_ordinal = ordinal;
        self.requests.insert(
            transport_id,
            ProviderRequestIdentity {
                canonical_request,
                call_id: call_id.clone(),
            },
        );
        Ok(call_id)
    }
}

/// Per-run cancel token registered by `arm_turn` before `send_turn`.
#[derive(Debug, Clone)]
pub struct ArmedTurn {
    pub cancel: Arc<AtomicBool>,
}

/// State while a prompt RPC is in flight.
pub struct ActivePrompt {
    pub run_id: RunId,
    pub acp_session_id: String,
    pub project_root: PathBuf,
    pub(crate) provider_requests: ProviderRequestIdentityRegistry,
    pub cancel: Arc<AtomicBool>,
    pub tools: ToolRegistry,
    pub assistant_chars: usize,
    pub sequence: u64,
    pub terminal_emitted: bool,
    /// Accept `session/update` only after the prompt request is written so
    /// late `session/load` replay cannot attach to the new turn.
    pub live: bool,
}

/// Registry of armed and active turns.
#[derive(Default)]
pub struct TurnRegistry {
    armed: HashMap<RunId, ArmedTurn>,
    active: Option<ActivePrompt>,
}

impl TurnRegistry {
    pub fn arm(&mut self, run_id: RunId) -> Result<(), RuntimeError> {
        if self.armed.contains_key(&run_id)
            || self
                .active
                .as_ref()
                .map(|a| a.run_id == run_id)
                .unwrap_or(false)
        {
            return Err(RuntimeError::RunAlreadyActive { run_id });
        }
        self.armed.insert(
            run_id,
            ArmedTurn {
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        Ok(())
    }

    pub fn disarm(&mut self, run_id: RunId) {
        self.armed.remove(&run_id);
    }

    pub fn cancel(&mut self, run_id: RunId) -> Result<bool, RuntimeError> {
        if let Some(turn) = self.armed.get(&run_id) {
            turn.cancel.store(true, Ordering::SeqCst);
            return Ok(true);
        }
        if let Some(active) = self.active.as_ref()
            && active.run_id == run_id
        {
            active.cancel.store(true, Ordering::SeqCst);
            return Ok(true);
        }
        Err(RuntimeError::RunNotFound { run_id })
    }

    pub fn cancel_all(&mut self) {
        for turn in self.armed.values() {
            turn.cancel.store(true, Ordering::SeqCst);
        }
        if let Some(active) = self.active.as_ref() {
            active.cancel.store(true, Ordering::SeqCst);
        }
    }

    pub fn take_cancel_token(&mut self, run_id: RunId) -> Arc<AtomicBool> {
        if let Some(turn) = self.armed.remove(&run_id) {
            return turn.cancel;
        }
        Arc::new(AtomicBool::new(false))
    }

    pub fn begin_prompt(
        &mut self,
        run_id: RunId,
        acp_session_id: String,
        project_root: String,
        cancel: Arc<AtomicBool>,
    ) -> Result<(), RuntimeError> {
        if self.active.is_some() {
            return Err(RuntimeError::Internal {
                message: "acp_busy".into(),
            });
        }
        self.active = Some(ActivePrompt {
            run_id,
            acp_session_id: acp_session_id.clone(),
            project_root: PathBuf::from(&project_root),
            provider_requests: ProviderRequestIdentityRegistry::default(),
            cancel,
            tools: ToolRegistry::new(acp_session_id, project_root),
            assistant_chars: 0,
            sequence: 0,
            terminal_emitted: false,
            live: false,
        });
        Ok(())
    }

    pub fn mark_live(&mut self) {
        if let Some(active) = self.active.as_mut() {
            active.live = true;
        }
    }

    pub fn active_mut(&mut self) -> Option<&mut ActivePrompt> {
        self.active.as_mut()
    }

    pub fn active(&self) -> Option<&ActivePrompt> {
        self.active.as_ref()
    }

    pub fn end_prompt(&mut self) {
        self.active = None;
    }

    pub fn clear(&mut self) {
        self.cancel_all();
        self.armed.clear();
        self.active = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_request_identity_is_stable_and_conflicts_fail_closed() {
        let run_id = RunId::new();
        let mut registry = ProviderRequestIdentityRegistry::default();
        let call_id = registry
            .resolve(run_id, "n:41".into(), "fs.read:note".into())
            .unwrap();

        assert_eq!(
            registry
                .resolve(run_id, "n:41".into(), "fs.read:note".into())
                .unwrap(),
            call_id
        );
        assert_eq!(
            registry.resolve(run_id, "n:41".into(), "fs.read:other".into()),
            Err(ProviderRequestIdentityError::Conflict)
        );
        assert_ne!(call_id, "grok-acp:41");
        assert!(call_id.contains(&run_id.to_string()));
    }

    #[test]
    fn provider_request_identity_is_scoped_to_the_active_run() {
        let first_run = RunId::new();
        let second_run = RunId::new();
        let first = ProviderRequestIdentityRegistry::default()
            .resolve(first_run, "n:41".into(), "fs.read:note".into())
            .unwrap();
        let second = ProviderRequestIdentityRegistry::default()
            .resolve(second_run, "n:41".into(), "fs.read:note".into())
            .unwrap();

        assert_ne!(first, second);
    }
}
