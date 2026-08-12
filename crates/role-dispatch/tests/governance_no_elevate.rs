//! Ungoverned LocalCli targets must never be elevated to "governed".

use pillow_role_dispatch::{
    Assignment, AssignmentId, AssignmentState, AssignmentTarget, AttributeError,
    BuildAssignmentInput, GovernanceCapability, ProcessOutputRecord, SlotAuthShape, SlotId,
    SlotRole, TargetSlotRecord, attribute_outcome, attribute_outcome_for_assignment,
    build_assignment, build_proposed_command,
};
use uuid::Uuid;

fn slot(n: u8) -> SlotId {
    SlotId(Uuid::from_bytes([
        n, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    ]))
}

#[test]
fn local_cli_command_capability_is_none() {
    let auth = SlotAuthShape::LocalCli {
        executable_path: "external-cli".into(),
        launch_args: vec!["status".into()],
        status_args: vec!["--check".into()],
    };
    assert_eq!(auth.governance(), GovernanceCapability::None);
    let cmd = build_proposed_command(&auth, "record only", None).unwrap();
    assert_eq!(cmd.governance, GovernanceCapability::None);
    assert!(!cmd.governance.is_governed());
}

#[test]
fn governed_auth_shapes_are_full() {
    for auth in [
        SlotAuthShape::ApiKey {
            model_id: "m".into(),
        },
        SlotAuthShape::GrokLogin { model_id: None },
        SlotAuthShape::GrokLogin {
            model_id: Some("m".into()),
        },
    ] {
        assert_eq!(auth.governance(), GovernanceCapability::Full);
        let cmd = build_proposed_command(&auth, "task", None).unwrap();
        assert_eq!(cmd.governance, GovernanceCapability::Full);
    }
}

#[test]
fn attribute_rejects_claiming_governed_for_local_cli() {
    let err = attribute_outcome(pillow_role_dispatch::AttributeInput {
        assignment_id: AssignmentId::new(),
        initiator_slot_id: slot(1),
        target_slot_id: slot(2),
        governance: GovernanceCapability::None,
        output: ProcessOutputRecord {
            exit_code: Some(0),
            duration_ms: Some(12),
            ..Default::default()
        },
        claimed_governed_execution: true,
    })
    .expect_err("elevation");
    assert_eq!(err, AttributeError::GovernanceElevationForbidden);
    assert_eq!(err.code(), "role_dispatch_governance_elevation");
}

#[test]
fn attribute_allows_record_only_for_local_cli() {
    let out = attribute_outcome(pillow_role_dispatch::AttributeInput {
        assignment_id: AssignmentId::new(),
        initiator_slot_id: slot(1),
        target_slot_id: slot(2),
        governance: GovernanceCapability::None,
        output: ProcessOutputRecord {
            exit_code: Some(1),
            duration_ms: Some(50),
            stdout_digest: Some("sha256:abc".into()),
            stderr_digest: None,
            truncated: false,
            cancelled: false,
            timed_out: false,
        },
        claimed_governed_execution: false,
    })
    .expect("record");
    assert!(!out.claimed_governed_execution);
    assert_eq!(out.governance, GovernanceCapability::None);
    assert_eq!(out.output.exit_code, Some(1));
}

#[test]
fn attribute_allows_governed_claim_only_when_capability_is_full() {
    let out = attribute_outcome(pillow_role_dispatch::AttributeInput {
        assignment_id: AssignmentId::new(),
        initiator_slot_id: slot(1),
        target_slot_id: slot(3),
        governance: GovernanceCapability::Full,
        output: ProcessOutputRecord {
            exit_code: Some(0),
            duration_ms: Some(100),
            ..Default::default()
        },
        claimed_governed_execution: true,
    })
    .expect("governed ok");
    assert!(out.claimed_governed_execution);
    assert!(out.governance.is_governed());
}

#[test]
fn assignment_freezes_local_cli_governance_and_blocks_elevation() {
    let new = build_assignment(BuildAssignmentInput {
        initiator_slot_id: slot(1),
        target: Some(TargetSlotRecord {
            slot_id: slot(2),
            role: SlotRole::Custom("worker".into()),
            auth: SlotAuthShape::LocalCli {
                executable_path: "cli".into(),
                launch_args: vec![],
                status_args: vec![],
            },
        }),
        requested_role: None,
        requested_slot_id: Some(slot(2)),
        task_text: "do work".into(),
        cwd: None,
    })
    .expect("build");

    assert_eq!(new.governance, GovernanceCapability::None);
    assert_eq!(new.proposed_command.governance, GovernanceCapability::None);

    let assignment = Assignment::from(new);
    let err = attribute_outcome_for_assignment(
        &assignment,
        ProcessOutputRecord {
            exit_code: Some(0),
            ..Default::default()
        },
        true, // illegal elevation claim
    )
    .expect_err("no elevate");
    assert_eq!(err, AttributeError::GovernanceElevationForbidden);
}

#[test]
fn outcome_attribution_carries_initiator_and_target() {
    let assignment = Assignment {
        id: AssignmentId::new(),
        initiator_slot_id: slot(10),
        target_slot_id: slot(20),
        target_role: SlotRole::Librarian,
        target_address: AssignmentTarget::Role(SlotRole::Librarian),
        task_text: "audit".into(),
        proposed_command: build_proposed_command(
            &SlotAuthShape::GrokLogin { model_id: None },
            "audit",
            None,
        )
        .unwrap(),
        governance: GovernanceCapability::Full,
        state: AssignmentState::Succeeded,
    };

    let out = attribute_outcome_for_assignment(
        &assignment,
        ProcessOutputRecord {
            exit_code: Some(0),
            duration_ms: Some(200),
            stdout_digest: Some("d1".into()),
            stderr_digest: Some("d2".into()),
            truncated: true,
            cancelled: false,
            timed_out: false,
        },
        true,
    )
    .expect("attr");

    assert_eq!(out.assignment_id, assignment.id);
    assert_eq!(out.initiator_slot_id, slot(10));
    assert_eq!(out.target_slot_id, slot(20));
    assert!(out.claimed_governed_execution);
    assert!(out.output.truncated);
}
