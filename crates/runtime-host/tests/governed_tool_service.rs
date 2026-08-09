//! Step 6 owner-scoped governance service composition and shutdown ownership.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use keeper_agent_protocol::{
    AgentRuntimeAdapter, EventSink, ProviderToolProposal, RunId, RuntimeError, RuntimeProbe,
    RuntimeSession, SendTurn, StartSession,
};
use keeper_governance_port::testkit::FixedCancellationProbe;
use keeper_governance_port::{GovernedExecutionPort, GovernedReadExecutionError};
use keeper_runtime_host::{
    FakeRuntimeAdapter, GovernedReadBridgeError, HostConfig, PreparedRuntimeHost, RuntimeHost,
    execute_governed_read,
};
use tempfile::tempdir;

fn invalid_read_proposal() -> ProviderToolProposal {
    ProviderToolProposal {
        call_id: "step6-service-probe".into(),
        tool_id: "invalid.tool".into(),
        tool_version: 1,
        arguments: serde_json::json!({}),
    }
}

fn fake_adapter() -> FakeRuntimeAdapter {
    FakeRuntimeAdapter::from_fixture_text(
        r#"
{"kind":"run_started","delay_ms":0,"payload":{}}
{"kind":"turn_completed","delay_ms":0,"payload":{}}
"#,
        false,
    )
    .expect("fake adapter")
}

#[test]
fn prepared_host_gives_adapter_and_host_the_same_service_allocation() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("keeper.db");
    let prepared = PreparedRuntimeHost::open(&db, HostConfig::default()).expect("prepared host");
    let execution = prepared.governed_execution_port();
    let adapter_service_ptr = Arc::as_ptr(&execution) as *const ();

    let host = prepared.finish(Arc::new(fake_adapter()));
    let host_service_ptr = host.governed_tool_service() as *const _ as *const ();

    assert_eq!(
        adapter_service_ptr, host_service_ptr,
        "prepared composition must erase and share the host's exact service Arc"
    );
    host.shutdown().expect("shutdown");
}

#[test]
fn service_execution_rejects_a_run_bound_by_another_owner() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("keeper.db");
    let prepared_a =
        PreparedRuntimeHost::open(&db, HostConfig::default()).expect("prepared host A");
    let prepared_b =
        PreparedRuntimeHost::open(&db, HostConfig::default()).expect("prepared host B");
    let execution_b = prepared_b.governed_execution_port();
    let host_a = prepared_a.finish(Arc::new(fake_adapter()));
    let host_b = prepared_b.finish(Arc::new(fake_adapter()));
    let run_id = RunId::new();
    host_a
        .register_governed_read_run(run_id, "project", "conversation", temp.path())
        .expect("bind owner A");

    assert!(
        matches!(
            execution_b.execute_read(
                run_id,
                invalid_read_proposal(),
                &FixedCancellationProbe::new()
            ),
            Err(GovernedReadExecutionError::RunNotBound)
        ),
        "owner B must not reach owner A's ambient compatibility binding"
    );

    host_a.shutdown().expect("shutdown A");
    host_b.shutdown().expect("shutdown B");
}

#[test]
fn failed_adapter_factory_closes_the_prepared_service_even_if_a_port_clone_escaped() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("keeper.db");
    let prepared = PreparedRuntimeHost::open(&db, HostConfig::default()).expect("prepared host");
    let escaped: Arc<Mutex<Option<Arc<dyn GovernedExecutionPort>>>> = Arc::new(Mutex::new(None));
    let escaped_from_factory = Arc::clone(&escaped);

    let result: Result<RuntimeHost, &'static str> =
        prepared.finish_with_adapter_factory(move |execution| {
            *escaped_from_factory.lock().expect("escaped port") = Some(execution);
            Err("adapter construction failed")
        });
    assert!(result.is_err(), "factory failure must surface");

    let execution = escaped
        .lock()
        .expect("escaped port")
        .take()
        .expect("factory received port");
    let error = execution
        .execute_read(
            RunId::new(),
            invalid_read_proposal(),
            &FixedCancellationProbe::new(),
        )
        .expect_err("prepared drop must close escaped execution view");
    assert_eq!(error, GovernedReadExecutionError::Unavailable);
}

struct ShutdownOrderAdapter {
    inner: FakeRuntimeAdapter,
    governance: Arc<dyn GovernedExecutionPort>,
    observed_service_open: Arc<AtomicBool>,
}

impl AgentRuntimeAdapter for ShutdownOrderAdapter {
    fn probe(&self) -> Result<RuntimeProbe, RuntimeError> {
        self.inner.probe()
    }

    fn start_session(&self, request: StartSession) -> Result<RuntimeSession, RuntimeError> {
        self.inner.start_session(request)
    }

    fn arm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.arm_turn(run_id)
    }

    fn disarm_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.disarm_turn(run_id)
    }

    fn send_turn(&self, request: SendTurn, sink: &mut dyn EventSink) -> Result<(), RuntimeError> {
        self.inner.send_turn(request, sink)
    }

    fn cancel_turn(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.inner.cancel_turn(run_id)
    }

    fn shutdown(&self) -> Result<(), RuntimeError> {
        let result = self.governance.execute_read(
            RunId::new(),
            invalid_read_proposal(),
            &FixedCancellationProbe::new(),
        );
        self.observed_service_open.store(
            matches!(result, Err(GovernedReadExecutionError::RunNotBound)),
            Ordering::SeqCst,
        );
        self.inner.shutdown()
    }
}

#[test]
fn host_shutdown_stops_adapter_before_releasing_and_closing_governed_service() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("keeper.db");
    let prepared = PreparedRuntimeHost::open(&db, HostConfig::default()).expect("prepared host");
    let execution = prepared.governed_execution_port();
    let observed_service_open = Arc::new(AtomicBool::new(false));
    let adapter = Arc::new(ShutdownOrderAdapter {
        inner: fake_adapter(),
        governance: Arc::clone(&execution),
        observed_service_open: Arc::clone(&observed_service_open),
    });
    let host = prepared.finish(adapter as Arc<dyn AgentRuntimeAdapter>);

    let run_id = RunId::new();
    host.register_governed_read_run(run_id, "project", "conversation", temp.path())
        .expect("bind read");
    assert!(!matches!(
        execute_governed_read(
            run_id,
            invalid_read_proposal(),
            &FixedCancellationProbe::new()
        ),
        Err(GovernedReadBridgeError::RunNotRegistered)
    ));

    host.shutdown().expect("host shutdown");

    assert!(
        observed_service_open.load(Ordering::SeqCst),
        "adapter shutdown must run while governed execution is still open"
    );
    assert!(
        matches!(
            execution.execute_read(
                RunId::new(),
                invalid_read_proposal(),
                &FixedCancellationProbe::new()
            ),
            Err(GovernedReadExecutionError::Unavailable)
        ),
        "service must reject new execution after host shutdown"
    );
    assert!(
        matches!(
            execute_governed_read(
                run_id,
                invalid_read_proposal(),
                &FixedCancellationProbe::new()
            ),
            Err(GovernedReadBridgeError::RunNotRegistered)
        ),
        "owner-wide release must happen before shutdown returns"
    );
}

#[test]
fn dropping_host_without_explicit_shutdown_still_closes_the_shared_service() {
    let temp = tempdir().expect("tempdir");
    let db = temp.path().join("keeper.db");
    let prepared = PreparedRuntimeHost::open(&db, HostConfig::default()).expect("prepared host");
    let execution = prepared.governed_execution_port();
    let host = prepared.finish(Arc::new(fake_adapter()));

    drop(host);

    assert!(matches!(
        execution.execute_read(
            RunId::new(),
            invalid_read_proposal(),
            &FixedCancellationProbe::new()
        ),
        Err(GovernedReadExecutionError::Unavailable)
    ));
}
