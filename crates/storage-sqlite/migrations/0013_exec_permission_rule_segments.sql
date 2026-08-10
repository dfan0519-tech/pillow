-- Ordered, per-segment process-rule attribution for conjunctive shell approval.
-- The existing one-to-one receipt remains the backward-compatible display
-- projection; these rows are the complete durable audit authority.

CREATE TABLE IF NOT EXISTS exec_permission_rule_segment_receipts (
    proposal_id TEXT NOT NULL
        REFERENCES exec_approval_decisions(proposal_id),
    segment_index INTEGER NOT NULL
        CHECK (segment_index >= 0 AND segment_index <= 4294967295),
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
    PRIMARY KEY (proposal_id, segment_index),
    CHECK (
        (rule_scope = 'global' AND rule_project_id IS NULL)
        OR (rule_scope = 'project' AND rule_project_id IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_exec_permission_rule_segment_receipts_rule_id
    ON exec_permission_rule_segment_receipts(rule_id);
