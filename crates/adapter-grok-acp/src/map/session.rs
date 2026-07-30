//! RuntimeSessionId ↔ private ACP sessionId mapping.

use std::collections::HashMap;

use keeper_agent_protocol::RuntimeSessionId;

#[derive(Debug, Clone)]
pub struct SessionRecord {
    #[allow(dead_code)]
    pub conversation_key: String,
    pub acp_session_id: String,
    #[allow(dead_code)]
    pub working_directory: String,
}

/// Private map: host session ids never expose ACP ids upward.
#[derive(Debug, Default)]
pub struct SessionMap {
    by_runtime: HashMap<RuntimeSessionId, SessionRecord>,
    by_acp: HashMap<String, RuntimeSessionId>,
}

impl SessionMap {
    pub fn insert(
        &mut self,
        runtime_id: RuntimeSessionId,
        conversation_key: String,
        acp_session_id: String,
        working_directory: String,
    ) {
        self.by_acp.insert(acp_session_id.clone(), runtime_id);
        self.by_runtime.insert(
            runtime_id,
            SessionRecord {
                conversation_key,
                acp_session_id,
                working_directory,
            },
        );
    }

    pub fn get(&self, runtime_id: &RuntimeSessionId) -> Option<&SessionRecord> {
        self.by_runtime.get(runtime_id)
    }

    pub fn clear(&mut self) {
        self.by_runtime.clear();
        self.by_acp.clear();
    }
}
