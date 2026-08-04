-- C3 durable write-proposal ledger. Stores identity, digests, lifecycle state,
-- decisions and terminal receipts only — never proposed/base content or raw
-- approval grants.

-- Canonical content/grant/fingerprint digests: "sha256:" + 64 lowercase hex.
-- Byte caps match agent-protocol WRITE_CONTENT_BYTE_CAP (262144).
-- Terminal result_json is required only for succeeded/failed: bounded,
-- non-empty, valid JSON. Every other state must keep result_json NULL.

CREATE TABLE IF NOT EXISTS write_proposals (
    proposal_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(proposal_id) = 36),
    reservation_key TEXT NOT NULL UNIQUE
        CHECK (length(reservation_key) >= 1 AND length(reservation_key) <= 512),
    proposal_fingerprint TEXT NOT NULL
        CHECK (
            length(proposal_fingerprint) = 71
            AND substr(proposal_fingerprint, 1, 7) = 'sha256:'
            AND substr(proposal_fingerprint, 8) NOT GLOB '*[^0-9a-f]*'
        ),
    project_id TEXT NOT NULL REFERENCES projects(id),
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    run_id TEXT NOT NULL REFERENCES runs(id),
    call_id TEXT NOT NULL
        CHECK (length(call_id) >= 1 AND length(call_id) <= 128),
    path TEXT NOT NULL
        CHECK (length(path) >= 1 AND length(path) <= 4096),
    mode TEXT NOT NULL
        CHECK (mode IN ('create', 'modify')),
    proposed_digest TEXT NOT NULL
        CHECK (
            length(proposed_digest) = 71
            AND substr(proposed_digest, 1, 7) = 'sha256:'
            AND substr(proposed_digest, 8) NOT GLOB '*[^0-9a-f]*'
        ),
    proposed_bytes INTEGER NOT NULL
        CHECK (proposed_bytes >= 1 AND proposed_bytes <= 262144),
    base_digest TEXT
        CHECK (
            base_digest IS NULL
            OR (
                length(base_digest) = 71
                AND substr(base_digest, 1, 7) = 'sha256:'
                AND substr(base_digest, 8) NOT GLOB '*[^0-9a-f]*'
            )
        ),
    base_bytes INTEGER NOT NULL
        CHECK (base_bytes >= 0 AND base_bytes <= 262144),
    state TEXT NOT NULL
        CHECK (state IN (
            'pending_approval',
            'approved',
            'applying',
            'succeeded',
            'rejected',
            'expired',
            'cancelled',
            'interrupted',
            'failed'
        )),
    expires_at TEXT NOT NULL,
    result_json TEXT,
    apply_owner TEXT
        CHECK (
            apply_owner IS NULL
            OR (length(apply_owner) >= 1 AND length(apply_owner) <= 128)
        ),
    apply_lease_expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, conversation_id, run_id, call_id),
    CHECK (
        (mode = 'create' AND base_digest IS NULL AND base_bytes = 0)
        OR (mode = 'modify' AND base_digest IS NOT NULL)
    ),
    -- Apply terminals require bounded valid JSON; every other state must be null.
    CHECK (
        (state IN ('succeeded', 'failed')
            AND result_json IS NOT NULL
            AND length(result_json) >= 1
            AND length(result_json) <= 8192
            AND json_valid(result_json))
        OR (state NOT IN ('succeeded', 'failed') AND result_json IS NULL)
    )
);

-- At most one pending/approved/applying proposal per run.
CREATE UNIQUE INDEX IF NOT EXISTS idx_write_proposals_one_active_per_run
    ON write_proposals(run_id)
    WHERE state IN ('pending_approval', 'approved', 'applying');

CREATE INDEX IF NOT EXISTS idx_write_proposals_conversation_created
    ON write_proposals(conversation_id, created_at ASC, proposal_id ASC);

CREATE INDEX IF NOT EXISTS idx_write_proposals_state
    ON write_proposals(state);

CREATE TABLE IF NOT EXISTS write_proposal_transitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proposal_id TEXT NOT NULL REFERENCES write_proposals(proposal_id),
    state TEXT NOT NULL
        CHECK (state IN (
            'pending_approval',
            'approved',
            'applying',
            'succeeded',
            'rejected',
            'expired',
            'cancelled',
            'interrupted',
            'failed'
        )),
    result_json TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (proposal_id, state),
    -- Same state/result rule as write_proposals: apply terminals require
    -- bounded valid JSON; every other transition requires null.
    CHECK (
        (state IN ('succeeded', 'failed')
            AND result_json IS NOT NULL
            AND length(result_json) >= 1
            AND length(result_json) <= 8192
            AND json_valid(result_json))
        OR (state NOT IN ('succeeded', 'failed') AND result_json IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_write_proposal_transitions_order
    ON write_proposal_transitions(proposal_id, id ASC);

CREATE TABLE IF NOT EXISTS write_approval_decisions (
    proposal_id TEXT PRIMARY KEY NOT NULL REFERENCES write_proposals(proposal_id),
    decision TEXT NOT NULL
        CHECK (decision IN ('approve_once', 'reject')),
    approval_grant_digest TEXT NOT NULL UNIQUE
        CHECK (
            length(approval_grant_digest) = 71
            AND substr(approval_grant_digest, 1, 7) = 'sha256:'
            AND substr(approval_grant_digest, 8) NOT GLOB '*[^0-9a-f]*'
        ),
    proposal_fingerprint TEXT NOT NULL
        CHECK (
            length(proposal_fingerprint) = 71
            AND substr(proposal_fingerprint, 1, 7) = 'sha256:'
            AND substr(proposal_fingerprint, 8) NOT GLOB '*[^0-9a-f]*'
        ),
    decided_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_write_approval_decisions_grant
    ON write_approval_decisions(approval_grant_digest);
