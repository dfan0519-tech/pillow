//! Tauri commands for controller → role dispatch into the governed process chain.
//!
//! Slot resolution and auth mapping happen here. Process reserve/approval uses
//! the same host path as agent `process.run@1` — never a side executor.

use keeper_storage_sqlite::{SlotAuth, SlotRole as StorageSlotRole};
use pillow_role_dispatch::{
    BuildAssignmentInput, SlotAuthShape, SlotId, SlotRole, TargetSlotRecord,
};
use pillow_runtime_slots::{RuntimeSlotService, RuntimeSlotView};
use tauri::{State, WebviewWindow};
use uuid::Uuid;

use crate::app_state::{AppState, CommandError};
use crate::conversation_service::{
    ConversationCommandError, RoleDispatchProposalDto, RoleDispatchRecordDto,
};
use crate::runtime_service_state::{RuntimeServiceState, require_project_scope};

fn map_error(error: ConversationCommandError) -> CommandError {
    CommandError {
        code: error.code,
        message: error.message,
    }
}

/// Propose a role-dispatch: target slot (and/or role) + task text.
///
/// Returns a live process proposal view in `pending_approval`. Does **not**
/// execute until a human decides via `decide_exec_proposal`.
/// Arity is fixed by Tauri: each injected `State` is its own parameter, so
/// grouping them would mean inventing a struct the framework cannot fill.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn propose_role_dispatch<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, RuntimeServiceState>,
    slots: State<'_, RuntimeSlotService>,
    window: WebviewWindow<R>,
    conversation_id: String,
    initiator_slot_id: String,
    target_slot_id: Option<String>,
    target_role: Option<String>,
    task_text: String,
) -> Result<RoleDispatchProposalDto, CommandError> {
    propose_role_dispatch_impl(
        &state,
        &service,
        &slots,
        window.label(),
        conversation_id,
        initiator_slot_id,
        target_slot_id,
        target_role,
        task_text,
    )
}

/// List in-memory dispatch records for a conversation (this host instance).
#[tauri::command]
pub fn list_role_dispatches<R: tauri::Runtime>(
    state: State<'_, AppState>,
    service: State<'_, RuntimeServiceState>,
    window: WebviewWindow<R>,
    conversation_id: String,
) -> Result<Vec<RoleDispatchRecordDto>, CommandError> {
    service
        .with_service(|svc, active_project_id| {
            let (id, project_id, root) = svc.conversation_project_context(&conversation_id)?;
            require_project_scope(active_project_id, &project_id.0.to_string())?;
            let _ = state
                .require_conversation_root_for_window(window.label(), project_id, id, &root)
                .map_err(command_to_conversation)?;
            svc.list_role_dispatches(conversation_id)
        })
        .map_err(map_error)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn propose_role_dispatch_impl(
    state: &AppState,
    service: &RuntimeServiceState,
    slots: &RuntimeSlotService,
    window_label: &str,
    conversation_id: String,
    initiator_slot_id: String,
    target_slot_id: Option<String>,
    target_role: Option<String>,
    task_text: String,
) -> Result<RoleDispatchProposalDto, CommandError> {
    service
        .with_service(|svc, active_project_id| {
            let (id, project_id, root) = svc.conversation_project_context(&conversation_id)?;
            require_project_scope(active_project_id, &project_id.0.to_string())?;
            let _ = state
                .require_conversation_root_for_window(window_label, project_id, id, &root)
                .map_err(command_to_conversation)?;

            let project_slots = slots
                .list(project_id)
                .map_err(|error| ConversationCommandError::new(error.code(), error.to_string()))?;

            let input = build_assignment_input(
                &project_slots,
                &initiator_slot_id,
                target_slot_id.as_deref(),
                target_role.as_deref(),
                task_text,
                Some(".".into()),
            )?;

            svc.propose_role_dispatch(conversation_id, input)
        })
        .map_err(map_error)
}

fn build_assignment_input(
    project_slots: &[RuntimeSlotView],
    initiator_slot_id: &str,
    target_slot_id: Option<&str>,
    target_role: Option<&str>,
    task_text: String,
    cwd: Option<String>,
) -> Result<BuildAssignmentInput, ConversationCommandError> {
    let requested_slot_id = target_slot_id.map(parse_dispatch_slot_id).transpose()?;
    let requested_role = target_role.map(parse_dispatch_role).transpose()?;

    if requested_slot_id.is_none() && requested_role.is_none() {
        return Err(ConversationCommandError::new(
            "role_dispatch_target_missing",
            "role dispatch target slot was not found",
        ));
    }

    // The Tauri payload only *claims* an initiator. Resolve it from the current
    // project's host-loaded slots so downstream domain state and the ledger
    // receive a host-derived Controller id, never the raw renderer value.
    let initiator = resolve_controller_initiator(project_slots, initiator_slot_id)?;

    let target_view =
        resolve_target_view(project_slots, requested_slot_id, requested_role.as_ref());
    let target = match target_view {
        Some(view) => Some(target_record_from_view(view)?),
        None => None,
    };

    Ok(BuildAssignmentInput {
        initiator_slot_id: initiator,
        target,
        requested_role,
        requested_slot_id,
        task_text,
        cwd,
    })
}

/// Resolve the claimed initiator against the slots loaded for the current
/// project. `RuntimeSlotService::list(project_id)` is deliberately the source
/// of truth: an arbitrary UUID is not a dispatch authority.
fn resolve_controller_initiator(
    project_slots: &[RuntimeSlotView],
    claimed_slot_id: &str,
) -> Result<SlotId, ConversationCommandError> {
    let claimed = parse_dispatch_initiator_slot_id(claimed_slot_id)?;
    let Some(view) = project_slots
        .iter()
        .find(|view| view.slot.id.0 == claimed.0)
    else {
        return Err(ConversationCommandError::new(
            "role_dispatch_initiator_not_in_project",
            "role dispatch initiator slot does not belong to the current project",
        ));
    };

    if !matches!(view.slot.role, StorageSlotRole::Controller) {
        return Err(ConversationCommandError::new(
            "role_dispatch_initiator_not_controller",
            "role dispatch initiator slot is not the current project's Controller",
        ));
    }

    // Return the id from host-loaded slot metadata rather than the parsed wire
    // claim. Equality above binds the claim; this value is what gets recorded.
    Ok(SlotId(view.slot.id.0))
}

fn resolve_target_view<'a>(
    project_slots: &'a [RuntimeSlotView],
    slot_id: Option<SlotId>,
    role: Option<&SlotRole>,
) -> Option<&'a RuntimeSlotView> {
    if let Some(id) = slot_id {
        return project_slots.iter().find(|view| view.slot.id.0 == id.0);
    }
    if let Some(role) = role {
        return project_slots
            .iter()
            .find(|view| storage_role_matches(&view.slot.role, role));
    }
    None
}

fn storage_role_matches(storage: &StorageSlotRole, wanted: &SlotRole) -> bool {
    match (storage, wanted) {
        (StorageSlotRole::Controller, SlotRole::Controller) => true,
        (StorageSlotRole::Librarian, SlotRole::Librarian) => true,
        (StorageSlotRole::Custom(a), SlotRole::Custom(b)) => a == b,
        _ => false,
    }
}

fn target_record_from_view(
    view: &RuntimeSlotView,
) -> Result<TargetSlotRecord, ConversationCommandError> {
    Ok(TargetSlotRecord {
        slot_id: SlotId(view.slot.id.0),
        role: map_storage_role(&view.slot.role),
        auth: map_slot_auth(&view.slot.auth)?,
    })
}

/// Map storage auth → domain shape. Unknown variants fail closed (no dispatch).
fn map_slot_auth(auth: &SlotAuth) -> Result<SlotAuthShape, ConversationCommandError> {
    match auth {
        SlotAuth::ApiKey { model_id, .. } => Ok(SlotAuthShape::ApiKey {
            model_id: model_id.clone(),
        }),
        SlotAuth::GrokLogin { model_id } => Ok(SlotAuthShape::GrokLogin {
            model_id: model_id.clone(),
        }),
        SlotAuth::LocalCli {
            executable_path,
            launch_args,
            status_args,
        } => Ok(SlotAuthShape::LocalCli {
            executable_path: executable_path.clone(),
            launch_args: launch_args.clone(),
            status_args: status_args.clone(),
        }),
        // Parallel OAuth / future auth variants: refuse dispatch.
        #[allow(unreachable_patterns)]
        _ => Err(ConversationCommandError::new(
            "role_dispatch_unsupported_auth",
            "dispatch target auth shape is unsupported",
        )),
    }
}

fn map_storage_role(role: &StorageSlotRole) -> SlotRole {
    match role {
        StorageSlotRole::Controller => SlotRole::Controller,
        StorageSlotRole::Librarian => SlotRole::Librarian,
        StorageSlotRole::Custom(name) => SlotRole::Custom(name.clone()),
        #[allow(unreachable_patterns)]
        _ => SlotRole::Custom("unknown".into()),
    }
}

fn parse_dispatch_slot_id(value: &str) -> Result<SlotId, ConversationCommandError> {
    Uuid::parse_str(value).map(SlotId).map_err(|_| {
        ConversationCommandError::new("runtime_slot_id_invalid", "Runtime slot id is invalid.")
    })
}

fn parse_dispatch_initiator_slot_id(value: &str) -> Result<SlotId, ConversationCommandError> {
    Uuid::parse_str(value).map(SlotId).map_err(|_| {
        ConversationCommandError::new(
            "role_dispatch_initiator_id_invalid",
            "role dispatch initiator slot id is invalid",
        )
    })
}

fn parse_dispatch_role(value: &str) -> Result<SlotRole, ConversationCommandError> {
    match value {
        "controller" => Ok(SlotRole::Controller),
        "librarian" => Ok(SlotRole::Librarian),
        other if !other.trim().is_empty() => {
            // Custom role name, or `custom:<name>` form.
            let name = other
                .strip_prefix("custom:")
                .unwrap_or(other)
                .trim()
                .to_owned();
            if name.is_empty() {
                return Err(ConversationCommandError::new(
                    "runtime_slot_input_invalid",
                    "Unknown role.",
                ));
            }
            Ok(SlotRole::Custom(name))
        }
        _ => Err(ConversationCommandError::new(
            "runtime_slot_input_invalid",
            "Unknown role.",
        )),
    }
}

fn command_to_conversation(error: CommandError) -> ConversationCommandError {
    ConversationCommandError::new(error.code, error.message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use keeper_project::ProjectId;
    // Imported here, not at module scope: only tests construct raw slot ids,
    // and clippy does not compile test code, so a module-level import reads as
    // unused and fails -D warnings while the tests still need it.
    use keeper_storage_sqlite::{RuntimeSlot, RuntimeSlotId, SlotAuth};
    use pillow_role_dispatch::{GovernanceCapability, ProposeError, build_assignment};

    fn slot_view(
        project_id: ProjectId,
        slot_id: Uuid,
        role: StorageSlotRole,
        auth: SlotAuth,
    ) -> RuntimeSlotView {
        RuntimeSlotView {
            slot: RuntimeSlot {
                id: RuntimeSlotId(slot_id),
                project_id,
                display_name: "test slot".into(),
                auth,
                role,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            secret_configured: true,
        }
    }

    fn controller_slot(project_id: ProjectId, slot_id: Uuid) -> RuntimeSlotView {
        slot_view(
            project_id,
            slot_id,
            StorageSlotRole::Controller,
            SlotAuth::GrokLogin { model_id: None },
        )
    }

    fn worker_slot(project_id: ProjectId, slot_id: Uuid) -> RuntimeSlotView {
        slot_view(
            project_id,
            slot_id,
            StorageSlotRole::Custom("worker".into()),
            SlotAuth::LocalCli {
                executable_path: "node".into(),
                launch_args: vec!["--version".into()],
                status_args: vec![],
            },
        )
    }

    fn sample_local(slot: Uuid) -> TargetSlotRecord {
        TargetSlotRecord {
            slot_id: SlotId(slot),
            role: SlotRole::Custom("worker".into()),
            auth: SlotAuthShape::LocalCli {
                executable_path: "node".into(),
                launch_args: vec!["--version".into()],
                status_args: vec![],
            },
        }
    }

    #[test]
    fn map_slot_auth_local_cli_is_ungoverned() {
        let auth = SlotAuth::LocalCli {
            executable_path: "node".into(),
            launch_args: vec![],
            status_args: vec![],
        };
        let shape = map_slot_auth(&auth).unwrap();
        assert_eq!(shape.governance(), GovernanceCapability::None);
    }

    #[test]
    fn map_slot_auth_api_key_is_full() {
        let auth = SlotAuth::ApiKey {
            base_url: "https://example.invalid/v1".into(),
            model_id: "m".into(),
            secret_ref: "ref".into(),
        };
        let shape = map_slot_auth(&auth).unwrap();
        assert_eq!(shape.governance(), GovernanceCapability::Full);
    }

    #[test]
    fn propose_error_codes_pass_through_prepare_path() {
        let id = SlotId::new();
        let err = build_assignment(BuildAssignmentInput {
            initiator_slot_id: id,
            target: Some(sample_local(id.0)),
            requested_role: None,
            requested_slot_id: Some(id),
            task_text: "x".into(),
            cwd: None,
        })
        .unwrap_err();
        assert!(matches!(err, ProposeError::SelfDispatch { .. }));
        assert_eq!(err.code(), "role_dispatch_self_dispatch");
    }

    #[test]
    fn target_missing_when_neither_slot_nor_role() {
        let err = build_assignment_input(
            &[],
            "00000000-0000-4000-8000-000000000001",
            None,
            None,
            "t".into(),
            None,
        )
        .unwrap_err();
        assert_eq!(err.code, "role_dispatch_target_missing");
    }

    #[test]
    fn project_controller_is_accepted_as_dispatch_initiator() {
        let project_id = ProjectId::new();
        let controller_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let project_slots = vec![
            controller_slot(project_id, controller_id),
            worker_slot(project_id, target_id),
        ];

        let input = build_assignment_input(
            &project_slots,
            &controller_id.to_string(),
            Some(&target_id.to_string()),
            None,
            "delegate this task".into(),
            Some(".".into()),
        )
        .expect("the current project's Controller must be able to dispatch");

        assert_eq!(input.initiator_slot_id, SlotId(controller_id));
        assert_eq!(input.target.expect("target").slot_id, SlotId(target_id));
    }

    #[test]
    fn random_initiator_id_is_rejected_at_native_dispatch_boundary() {
        let project_id = ProjectId::new();
        let controller_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let project_slots = vec![
            controller_slot(project_id, controller_id),
            worker_slot(project_id, target_id),
        ];

        let err = build_assignment_input(
            &project_slots,
            &Uuid::new_v4().to_string(),
            Some(&target_id.to_string()),
            None,
            "delegate this task".into(),
            Some(".".into()),
        )
        .expect_err("a random initiator must not enter the governed process chain");

        assert_eq!(err.code, "role_dispatch_initiator_not_in_project");
    }

    #[test]
    fn other_project_initiator_is_rejected_at_native_dispatch_boundary() {
        let project_id = ProjectId::new();
        let controller_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let project_slots = vec![
            controller_slot(project_id, controller_id),
            worker_slot(project_id, target_id),
        ];
        let other_project_controller = controller_slot(ProjectId::new(), Uuid::new_v4());

        let err = build_assignment_input(
            &project_slots,
            &other_project_controller.slot.id.0.to_string(),
            Some(&target_id.to_string()),
            None,
            "delegate this task".into(),
            Some(".".into()),
        )
        .expect_err("a Controller from another project must not dispatch here");

        assert_eq!(err.code, "role_dispatch_initiator_not_in_project");
    }

    #[test]
    fn non_controller_initiator_is_rejected_at_native_dispatch_boundary() {
        let project_id = ProjectId::new();
        let controller_id = Uuid::new_v4();
        let librarian_id = Uuid::new_v4();
        let target_id = Uuid::new_v4();
        let project_slots = vec![
            controller_slot(project_id, controller_id),
            slot_view(
                project_id,
                librarian_id,
                StorageSlotRole::Librarian,
                SlotAuth::GrokLogin { model_id: None },
            ),
            worker_slot(project_id, target_id),
        ];

        let err = build_assignment_input(
            &project_slots,
            &librarian_id.to_string(),
            Some(&target_id.to_string()),
            None,
            "delegate this task".into(),
            Some(".".into()),
        )
        .expect_err("a non-Controller initiator must be rejected");

        assert_eq!(err.code, "role_dispatch_initiator_not_controller");
    }

    #[test]
    fn controller_self_dispatch_remains_rejected_after_initiator_resolution() {
        let project_id = ProjectId::new();
        let controller_id = Uuid::new_v4();
        let project_slots = vec![controller_slot(project_id, controller_id)];

        let input = build_assignment_input(
            &project_slots,
            &controller_id.to_string(),
            Some(&controller_id.to_string()),
            None,
            "delegate this task".into(),
            Some(".".into()),
        )
        .expect("the Controller identity is valid before the domain self-dispatch check");

        let err = build_assignment(input).expect_err("self-dispatch must remain forbidden");
        assert!(matches!(err, ProposeError::SelfDispatch { .. }));
        assert_eq!(err.code(), "role_dispatch_self_dispatch");
    }

    #[test]
    fn _unused_runtime_slot_id_helper_keeps_storage_type_linked() {
        // Ensure RuntimeSlotId stays imported for future get-path wiring.
        let _ = RuntimeSlotId(Uuid::nil());
    }
}
