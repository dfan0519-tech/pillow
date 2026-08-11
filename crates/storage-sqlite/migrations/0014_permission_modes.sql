-- Global permission mode. The value remains in settings_entries; triggers make
-- the global-only invariant durable even if a caller bypasses typed Rust APIs.

CREATE TRIGGER IF NOT EXISTS trg_settings_permission_mode_global_insert
BEFORE INSERT ON settings_entries
WHEN NEW.key = 'permissions.mode'
 AND (NEW.scope <> 'global' OR NEW.project_id IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'permission mode is global-only');
END;

CREATE TRIGGER IF NOT EXISTS trg_settings_permission_mode_global_update
BEFORE UPDATE OF scope, project_id, key ON settings_entries
WHEN NEW.key = 'permissions.mode'
 AND (NEW.scope <> 'global' OR NEW.project_id IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'permission mode is global-only');
END;

-- Mode-attributed approvals remain separate from rule receipts: a mode is a
-- host-owned "do not ask" policy, not a matching allow rule. Both tables hang
-- off the durable approval decision so proposal/decision/execution receipts
-- remain intact and attribution cannot exist without a real decision.
CREATE TABLE IF NOT EXISTS write_permission_mode_receipts (
    proposal_id TEXT PRIMARY KEY NOT NULL
        REFERENCES write_approval_decisions(proposal_id),
    decision TEXT NOT NULL
        CHECK (decision = 'approve_by_mode'),
    actor TEXT NOT NULL
        CHECK (actor = 'pillow_permission_mode'),
    permission_mode TEXT NOT NULL
        CHECK (permission_mode IN ('accept_edits', 'bypass')),
    decided_at TEXT NOT NULL
        CHECK (length(decided_at) >= 1)
);

CREATE TABLE IF NOT EXISTS exec_permission_mode_receipts (
    proposal_id TEXT PRIMARY KEY NOT NULL
        REFERENCES exec_approval_decisions(proposal_id),
    decision TEXT NOT NULL
        CHECK (decision = 'approve_by_mode'),
    actor TEXT NOT NULL
        CHECK (actor = 'pillow_permission_mode'),
    permission_mode TEXT NOT NULL
        CHECK (permission_mode = 'bypass'),
    decided_at TEXT NOT NULL
        CHECK (length(decided_at) >= 1)
);
