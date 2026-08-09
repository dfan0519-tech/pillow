//! Two-phase runtime-host composition for adapters that require governance.

use std::path::Path;
use std::sync::Arc;

use keeper_agent_protocol::AgentRuntimeAdapter;
use keeper_governance_port::GovernedExecutionPort;

use super::{HostConfig, RuntimeHost};
use crate::{GovernedReadBridgeError, GovernedToolService};

/// Factory invoked only after the host-owned governance service is prepared.
pub trait GovernedAdapterFactory {
    /// Adapter construction error propagated without reinterpretation.
    type Error;

    /// Build an adapter with the prepared service's erased execution view.
    fn build(
        self,
        governance: Arc<dyn GovernedExecutionPort>,
    ) -> Result<Arc<dyn AgentRuntimeAdapter>, Self::Error>;
}

impl<F, E> GovernedAdapterFactory for F
where
    F: FnOnce(Arc<dyn GovernedExecutionPort>) -> Result<Arc<dyn AgentRuntimeAdapter>, E>,
{
    type Error = E;

    fn build(
        self,
        governance: Arc<dyn GovernedExecutionPort>,
    ) -> Result<Arc<dyn AgentRuntimeAdapter>, Self::Error> {
        self(governance)
    }
}

/// Open governance before constructing an adapter, then finish one host with
/// both consumers sharing the same service allocation.
pub struct PreparedRuntimeHost {
    config: Option<HostConfig>,
    governed_tools: Option<Arc<GovernedToolService>>,
}

impl PreparedRuntimeHost {
    /// Open the owner-scoped service and retain the host configuration.
    pub fn open(
        database_path: impl AsRef<Path>,
        config: HostConfig,
    ) -> Result<Self, GovernedReadBridgeError> {
        Ok(Self {
            config: Some(config),
            governed_tools: Some(Arc::new(GovernedToolService::open(database_path)?)),
        })
    }

    /// Clone and erase the exact service allocation that will be owned by the
    /// finished RuntimeHost.
    pub fn governed_execution_port(&self) -> Arc<dyn GovernedExecutionPort> {
        Arc::clone(
            self.governed_tools
                .as_ref()
                .expect("prepared host service already consumed"),
        ) as Arc<dyn GovernedExecutionPort>
    }

    /// Finish composition with an already-built adapter.
    pub fn finish(mut self, adapter: Arc<dyn AgentRuntimeAdapter>) -> RuntimeHost {
        let config = self
            .config
            .take()
            .expect("prepared host config already consumed");
        let governed_tools = self
            .governed_tools
            .take()
            .expect("prepared host service already consumed");
        RuntimeHost::build(adapter, config, governed_tools)
    }

    /// Invoke an adapter factory with this prepared service and finish the host.
    /// Factory failure drops this prepared owner and closes any escaped port
    /// clones before returning the original error.
    pub fn finish_with_adapter_factory<F>(self, factory: F) -> Result<RuntimeHost, F::Error>
    where
        F: GovernedAdapterFactory,
    {
        let adapter = factory.build(self.governed_execution_port())?;
        Ok(self.finish(adapter))
    }
}

impl Drop for PreparedRuntimeHost {
    fn drop(&mut self) {
        if let Some(service) = self.governed_tools.as_ref() {
            service.shutdown();
        }
    }
}
