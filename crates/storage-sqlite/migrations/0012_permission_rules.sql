-- Permission-rule minimal slice. SQLite is the only durable authority.
-- CHECK constraints enforce shape and enum membership only; semantic value
-- validation (including project deny-only) is performed in Rust.

CREATE TABLE IF NOT EXISTS permission_rules (
    rule_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(rule_id) = 36),
    scope TEXT NOT NULL
        CHECK (scope IN ('global', 'project')),
    project_id TEXT NULL REFERENCES projects(id),
    prefix TEXT NOT NULL
        CHECK (prefix IN ('process', 'write', 'edit')),
    pattern TEXT NOT NULL
        CHECK (length(pattern) <= 4096),
    action TEXT NOT NULL
        CHECK (action IN ('allow', 'deny')),
    content_digest TEXT NOT NULL
        CHECK (
            length(content_digest) = 71
            AND substr(content_digest, 1, 7) = 'sha256:'
            AND substr(content_digest, 8) NOT GLOB '*[^0-9a-f]*'
        ),
    created_at TEXT NOT NULL
        CHECK (length(created_at) >= 1),
    CHECK (
        (scope = 'global' AND project_id IS NULL)
        OR (scope = 'project' AND project_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_permission_rules_global_order
    ON permission_rules(created_at ASC, rule_id ASC)
    WHERE scope = 'global';

CREATE INDEX IF NOT EXISTS idx_permission_rules_project_order
    ON permission_rules(project_id, created_at ASC, rule_id ASC)
    WHERE scope = 'project';

-- These one-to-one extension rows bind an existing C3/C4 decision to the
-- complete rule snapshot used by the host. No foreign key targets the live
-- rule row: deleting a rule must not erase historical attribution.
CREATE TABLE IF NOT EXISTS write_permission_rule_receipts (
    proposal_id TEXT PRIMARY KEY NOT NULL
        REFERENCES write_approval_decisions(proposal_id),
    decision TEXT NOT NULL
        CHECK (decision IN ('approve_by_rule', 'reject_by_rule')),
    actor TEXT NOT NULL
        CHECK (actor = 'rule_engine'),
    rule_id TEXT NOT NULL
        CHECK (length(rule_id) = 36),
    rule_scope TEXT NOT NULL
        CHECK (rule_scope IN ('global', 'project')),
    rule_project_id TEXT NULL,
    rule_prefix TEXT NOT NULL
        CHECK (rule_prefix IN ('write', 'edit')),
    rule_pattern TEXT NOT NULL
        CHECK (length(rule_pattern) <= 4096),
    rule_action TEXT NOT NULL
        CHECK (rule_action IN ('allow', 'deny')),
    rule_content_digest TEXT NOT NULL
        CHECK (
            length(rule_content_digest) = 71
            AND substr(rule_content_digest, 1, 7) = 'sha256:'
            AND substr(rule_content_digest, 8) NOT GLOB '*[^0-9a-f]*'
        ),
    match_subject TEXT NOT NULL
        CHECK (length(match_subject) >= 1 AND length(match_subject) <= 8192),
    decided_at TEXT NOT NULL
        CHECK (length(decided_at) >= 1),
    CHECK (
        (rule_scope = 'global' AND rule_project_id IS NULL)
        OR (rule_scope = 'project' AND rule_project_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_write_permission_rule_receipts_rule_id
    ON write_permission_rule_receipts(rule_id);

CREATE TABLE IF NOT EXISTS exec_permission_rule_receipts (
    proposal_id TEXT PRIMARY KEY NOT NULL
        REFERENCES exec_approval_decisions(proposal_id),
    decision TEXT NOT NULL
        CHECK (decision IN ('approve_by_rule', 'reject_by_rule')),
    actor TEXT NOT NULL
        CHECK (actor = 'rule_engine'),
    rule_id TEXT NOT NULL
        CHECK (length(rule_id) = 36),
    rule_scope TEXT NOT NULL
        CHECK (rule_scope IN ('global', 'project')),
    rule_project_id TEXT NULL,
    rule_prefix TEXT NOT NULL
        CHECK (rule_prefix = 'process'),
    rule_pattern TEXT NOT NULL
        CHECK (length(rule_pattern) <= 4096),
    rule_action TEXT NOT NULL
        CHECK (rule_action IN ('allow', 'deny')),
    rule_content_digest TEXT NOT NULL
        CHECK (
            length(rule_content_digest) = 71
            AND substr(rule_content_digest, 1, 7) = 'sha256:'
            AND substr(rule_content_digest, 8) NOT GLOB '*[^0-9a-f]*'
        ),
    match_subject TEXT NOT NULL
        CHECK (length(match_subject) >= 1 AND length(match_subject) <= 8192),
    decided_at TEXT NOT NULL
        CHECK (length(decided_at) >= 1),
    CHECK (
        (rule_scope = 'global' AND rule_project_id IS NULL)
        OR (rule_scope = 'project' AND rule_project_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_exec_permission_rule_receipts_rule_id
    ON exec_permission_rule_receipts(rule_id);
