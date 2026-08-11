//! Replaceable conversation runtime owned below the renderer command layer.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use keeper_adapter_grok_acp::GrokAcpConfig;

use crate::conversation_events::RuntimeEventEmitter;
use crate::conversation_service::{ConversationCommandError, ConversationService};
use crate::runtime_composition::open_conversation_service_with_grok_config;

struct RuntimeServiceInner {
    service: Option<Arc<ConversationService>>,
    active_project_id: Option<String>,
    active_slot_id: Option<String>,
}

pub struct RuntimeServiceState {
    database_path: PathBuf,
    emitter: Arc<dyn RuntimeEventEmitter>,
    slot_lifecycle: Mutex<()>,
    inner: Mutex<RuntimeServiceInner>,
}

impl RuntimeServiceState {
    pub fn new(
        database_path: impl Into<PathBuf>,
        emitter: Arc<dyn RuntimeEventEmitter>,
        service: Option<ConversationService>,
    ) -> Self {
        Self {
            database_path: database_path.into(),
            emitter,
            slot_lifecycle: Mutex::new(()),
            inner: Mutex::new(RuntimeServiceInner {
                service: service.map(Arc::new),
                active_project_id: None,
                active_slot_id: None,
            }),
        }
    }

    pub fn with_service<T>(
        &self,
        operation: impl FnOnce(
            &ConversationService,
            Option<&str>,
        ) -> Result<T, ConversationCommandError>,
    ) -> Result<T, ConversationCommandError> {
        let inner = self.inner.lock().map_err(|_| state_unavailable())?;
        let service = inner.service.as_ref().ok_or_else(runtime_unavailable)?;
        operation(service, inner.active_project_id.as_deref())
    }

    pub fn activate_grok(
        &self,
        project_id: String,
        slot_id: String,
        config: GrokAcpConfig,
    ) -> Result<(), ConversationCommandError> {
        // Construct first: a bad executable/config must leave the active service intact.
        let replacement = Arc::new(open_conversation_service_with_grok_config(
            &self.database_path,
            Arc::clone(&self.emitter),
            config,
        )?);
        let mut inner = self.inner.lock().map_err(|_| state_unavailable())?;
        if let Some(previous) = inner.service.take()
            && let Err(error) = previous.shutdown()
        {
            let _ = replacement.shutdown();
            inner.active_project_id = None;
            inner.active_slot_id = None;
            return Err(error);
        }
        if let Err(error) = replacement.recover_inflight() {
            let _ = replacement.shutdown();
            inner.active_project_id = None;
            inner.active_slot_id = None;
            return Err(error);
        }
        inner.service = Some(replacement);
        inner.active_project_id = Some(project_id);
        inner.active_slot_id = Some(slot_id);
        Ok(())
    }

    /// Serialize slot start/update/delete command lifecycles. The guard keeps a
    /// model metadata update and its required runtime replacement atomic with
    /// respect to another slot command.
    pub fn lock_slot_lifecycle(&self) -> Result<MutexGuard<'_, ()>, ConversationCommandError> {
        self.slot_lifecycle.lock().map_err(|_| state_unavailable())
    }

    pub fn is_active_slot(
        &self,
        project_id: &str,
        slot_id: &str,
    ) -> Result<bool, ConversationCommandError> {
        let inner = self.inner.lock().map_err(|_| state_unavailable())?;
        Ok(inner.active_project_id.as_deref() == Some(project_id)
            && inner.active_slot_id.as_deref() == Some(slot_id))
    }

    pub fn shutdown_if_active(&self) -> Result<bool, ConversationCommandError> {
        let service = {
            let mut inner = self.inner.lock().map_err(|_| state_unavailable())?;
            inner.active_project_id = None;
            inner.active_slot_id = None;
            inner.service.take()
        };
        match service {
            Some(service) => service.shutdown().map(|()| true),
            None => Ok(false),
        }
    }

    /// Shut down the conversation runtime only when the given project+slot is active.
    /// Used by slot delete so removing the active slot does not leave a dangling runtime.
    pub fn shutdown_if_active_slot(
        &self,
        project_id: &str,
        slot_id: &str,
    ) -> Result<bool, ConversationCommandError> {
        let is_active = {
            let inner = self.inner.lock().map_err(|_| state_unavailable())?;
            inner.active_project_id.as_deref() == Some(project_id)
                && inner.active_slot_id.as_deref() == Some(slot_id)
        };
        if is_active {
            self.shutdown_if_active()
        } else {
            Ok(false)
        }
    }
}

pub(crate) fn require_project_scope(
    active_project_id: Option<&str>,
    requested_project_id: &str,
) -> Result<(), ConversationCommandError> {
    if active_project_id.is_none_or(|active| active == requested_project_id) {
        return Ok(());
    }
    Err(ConversationCommandError::new(
        "runtime_slot_project_mismatch",
        "The active runtime slot belongs to a different project.",
    ))
}

fn runtime_unavailable() -> ConversationCommandError {
    ConversationCommandError::new(
        "runtime_unavailable",
        "Conversation runtime is unavailable. Start a governed runtime slot.",
    )
}

fn state_unavailable() -> ConversationCommandError {
    ConversationCommandError::new(
        "runtime_state_unavailable",
        "Conversation runtime state is unavailable.",
    )
}
