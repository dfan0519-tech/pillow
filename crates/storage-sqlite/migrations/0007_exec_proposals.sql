-- C4 durable process-proposal ledger. Stores identity, digests, lifecycle
-- state, decisions and terminal receipts only — never raw argv, native
-- executable/root/cwd, raw approval grants, arbitrary environment or
-- stdout/stderr transcript.

-- Canonical digests: "sha256:" + 64 lowercase hex.
-- Program labels match process.run@1: ^[a-z][a-z0-9_-]{0,31}$.
-- Terminal result_json is required for succeeded/failed (bounded, non-empty,
-- valid JSON). Post-spawn cancelled may carry the same bounded receipt and
-- terminal metadata; pre-spawn pending/approved cancellation stays receipt-free.
-- Every other state must keep result_json and terminal metadata NULL.

CREATE TABLE IF NOT EXISTS exec_proposals (
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
    program TEXT NOT NULL
        CHECK (
            length(program) >= 1 AND length(program) <= 32
            AND program GLOB '[a-z]*'
            AND program NOT GLOB '*[^a-z0-9_-]*'
        ),
    argv_digest TEXT NOT NULL
        CHECK (
            length(argv_digest) = 71
            AND substr(argv_digest, 1, 7) = 'sha256:'
            AND substr(argv_digest, 8) NOT GLOB '*[^0-9a-f]*'
        ),
    portable_cwd TEXT
        CHECK (
            portable_cwd IS NULL
            OR (
                length(portable_cwd) >= 1
                AND length(portable_cwd) <= 4096
                AND portable_cwd NOT GLOB '*[\\:]*'
                AND portable_cwd NOT GLOB '/*'
            )
        ),
    state TEXT NOT NULL
        CHECK (state IN (
            'pending_approval',
            'approved',
            'running',
            'succeeded',
            'failed',
            'rejected',
            'expired',
            'cancelled',
            'interrupted'
        )),
    expires_at TEXT NOT NULL,
    result_json TEXT,
    run_owner TEXT
        CHECK (
            run_owner IS NULL
            OR (length(run_owner) >= 1 AND length(run_owner) <= 128)
        ),
    run_lease_expires_at TEXT,
    started_at TEXT,
    completed_at TEXT,
    exit_code INTEGER,
    duration_ms INTEGER
        CHECK (duration_ms IS NULL OR duration_ms >= 0),
    stdout_digest TEXT
        CHECK (
            stdout_digest IS NULL
            OR (
                length(stdout_digest) = 71
                AND substr(stdout_digest, 1, 7) = 'sha256:'
                AND substr(stdout_digest, 8) NOT GLOB '*[^0-9a-f]*'
            )
        ),
    stderr_digest TEXT
        CHECK (
            stderr_digest IS NULL
            OR (
                length(stderr_digest) = 71
                AND substr(stderr_digest, 1, 7) = 'sha256:'
                AND substr(stderr_digest, 8) NOT GLOB '*[^0-9a-f]*'
            )
        ),
    captured_bytes INTEGER
        CHECK (captured_bytes IS NULL OR captured_bytes >= 0),
    truncated INTEGER
        CHECK (truncated IS NULL OR truncated IN (0, 1)),
    encoding_lossy INTEGER
        CHECK (encoding_lossy IS NULL OR encoding_lossy IN (0, 1)),
    cancelled INTEGER
        CHECK (cancelled IS NULL OR cancelled IN (0, 1)),
    timed_out INTEGER
        CHECK (timed_out IS NULL OR timed_out IN (0, 1)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, conversation_id, run_id, call_id),
    -- Apply terminals require bounded valid JSON and full terminal metadata.
    -- Cancelled may be receipt-bearing (post-spawn) or receipt-free (pre-spawn).
    -- Every other state must keep result_json and terminal metadata null.
    CHECK (
        (state IN ('succeeded', 'failed')
            AND result_json IS NOT NULL
            AND length(result_json) >= 1
            AND length(result_json) <= 8192
            AND json_valid(result_json)
            AND completed_at IS NOT NULL
            AND duration_ms IS NOT NULL
            AND stdout_digest IS NOT NULL
            AND stderr_digest IS NOT NULL
            AND captured_bytes IS NOT NULL
            AND truncated IS NOT NULL
            AND encoding_lossy IS NOT NULL
            AND cancelled IS NOT NULL
            AND timed_out IS NOT NULL)
        OR (state = 'cancelled'
            AND (
                (result_json IS NOT NULL
                    AND length(result_json) >= 1
                    AND length(result_json) <= 8192
                    AND json_valid(result_json)
                    AND completed_at IS NOT NULL
                    AND duration_ms IS NOT NULL
                    AND stdout_digest IS NOT NULL
                    AND stderr_digest IS NOT NULL
                    AND captured_bytes IS NOT NULL
                    AND truncated IS NOT NULL
                    AND encoding_lossy IS NOT NULL
                    AND cancelled IS NOT NULL
                    AND timed_out IS NOT NULL)
                OR (result_json IS NULL
                    AND exit_code IS NULL
                    AND duration_ms IS NULL
                    AND stdout_digest IS NULL
                    AND stderr_digest IS NULL
                    AND captured_bytes IS NULL
                    AND truncated IS NULL
                    AND encoding_lossy IS NULL
                    AND cancelled IS NULL
                    AND timed_out IS NULL
                    AND completed_at IS NULL)
            ))
        OR (state NOT IN ('succeeded', 'failed', 'cancelled')
            AND result_json IS NULL
            AND exit_code IS NULL
            AND duration_ms IS NULL
            AND stdout_digest IS NULL
            AND stderr_digest IS NULL
            AND captured_bytes IS NULL
            AND truncated IS NULL
            AND encoding_lossy IS NULL
            AND cancelled IS NULL
            AND timed_out IS NULL
            AND completed_at IS NULL)
    )
);

-- At most one pending/approved/running proposal per run.
CREATE UNIQUE INDEX IF NOT EXISTS idx_exec_proposals_one_active_per_run
    ON exec_proposals(run_id)
    WHERE state IN ('pending_approval', 'approved', 'running');

CREATE INDEX IF NOT EXISTS idx_exec_proposals_conversation_created
    ON exec_proposals(conversation_id, created_at ASC, proposal_id ASC);

CREATE INDEX IF NOT EXISTS idx_exec_proposals_state
    ON exec_proposals(state);

CREATE TABLE IF NOT EXISTS exec_proposal_transitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    proposal_id TEXT NOT NULL REFERENCES exec_proposals(proposal_id),
    state TEXT NOT NULL
        CHECK (state IN (
            'pending_approval',
            'approved',
            'running',
            'succeeded',
            'failed',
            'rejected',
            'expired',
            'cancelled',
            'interrupted'
        )),
    result_json TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (proposal_id, state),
    CHECK (
        (state IN ('succeeded', 'failed')
            AND result_json IS NOT NULL
            AND length(result_json) >= 1
            AND length(result_json) <= 8192
            AND json_valid(result_json))
        OR (state = 'cancelled'
            AND (
                (result_json IS NOT NULL
                    AND length(result_json) >= 1
                    AND length(result_json) <= 8192
                    AND json_valid(result_json))
                OR result_json IS NULL
            ))
        OR (state NOT IN ('succeeded', 'failed', 'cancelled') AND result_json IS NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_exec_proposal_transitions_order
    ON exec_proposal_transitions(proposal_id, id ASC);

CREATE TABLE IF NOT EXISTS exec_approval_decisions (
    proposal_id TEXT PRIMARY KEY NOT NULL REFERENCES exec_proposals(proposal_id),
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

CREATE INDEX IF NOT EXISTS idx_exec_approval_decisions_grant
    ON exec_approval_decisions(approval_grant_digest);
