//! C4 typed Tauri commands for conversation-owned process views.

/// Fixed public-error vocabulary mirrored from `apps/desktop/src/lib/execGateway.ts`
/// (`EXEC_PUBLIC_ERRORS`). Private until product Tauri commands land (D3B scope).
#[allow(dead_code)] // exercised by unit tests; product commands are out of D3AR2C scope
fn is_exec_public_error(code: &str) -> bool {
    matches!(
        code,
        "process_ownership_mismatch"
            | "process_invalid_decision"
            | "process_not_live"
            | "process_expired"
            | "process_unavailable"
            | "process_output_unavailable"
            | "process_cleanup_unproven"
            | "process_path_rejected"
            | "process_conflict"
    )
}

#[cfg(test)]
mod c4_gateway_tests {
    #[test]
    fn c4_gateway_rejects_invalid_public_error_enum() {
        assert!(super::is_exec_public_error("process_unavailable"));
        assert!(!super::is_exec_public_error("native_error"));
    }
}
