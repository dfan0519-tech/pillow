//! Native role-dispatch command registration and error-code tests.

#[test]
fn desktop_invoke_handler_registers_role_dispatch_commands() {
    let source = include_str!("lib.rs");
    let handler = source
        .split("tauri::generate_handler![")
        .nth(1)
        .expect("desktop invoke handler must exist");
    for command in ["propose_role_dispatch", "list_role_dispatches"] {
        assert!(
            handler.contains(command),
            "missing invoke command {command}"
        );
    }
}

#[test]
fn propose_error_codes_are_stable_public_tokens() {
    // Mirrored from pillow-role-dispatch::ProposeError::code — the desktop
    // command must surface these unchanged (not collapsed to process_*).
    for code in [
        "role_dispatch_self_dispatch",
        "role_dispatch_target_missing",
        "role_dispatch_role_mismatch",
        "role_dispatch_shell_join_forbidden",
        "role_dispatch_task_empty",
        "role_dispatch_task_in_program_or_cwd",
        "role_dispatch_program_empty",
        "role_dispatch_local_cli_executable_empty",
        "role_dispatch_empty_argv_element",
        "role_dispatch_unsupported_auth",
    ] {
        assert!(
            code.starts_with("role_dispatch_"),
            "dispatch public codes stay namespaced: {code}"
        );
    }
}
