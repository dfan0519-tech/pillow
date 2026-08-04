-- B3 durable tool-call ledger. Reservation identity is separate from proposal
-- semantics so retries can replay an exact receipt and reject call-id reuse.

CREATE TABLE IF NOT EXISTS tool_calls (
    reservation_key TEXT PRIMARY KEY NOT NULL,
    proposal_fingerprint TEXT NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    run_id TEXT NOT NULL REFERENCES runs(id),
    call_id TEXT NOT NULL,
    tool_id TEXT NOT NULL
        CHECK (tool_id IN ('fs.list', 'fs.read', 'fs.search')),
    tool_version INTEGER NOT NULL
        CHECK (tool_version = 1),
    arguments_digest TEXT NOT NULL,
    proposal_json TEXT NOT NULL,
    state TEXT NOT NULL
        CHECK (state IN (
            'proposed',
            'validated',
            'executing',
            'succeeded',
            'failed',
            'cancelled'
        )),
    result_json TEXT,
    execution_owner TEXT,
    lease_expires_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (project_id, conversation_id, run_id, call_id)
);

CREATE INDEX IF NOT EXISTS idx_tool_calls_run_created
    ON tool_calls(run_id, created_at ASC, reservation_key ASC);

CREATE INDEX IF NOT EXISTS idx_tool_calls_state
    ON tool_calls(state);

CREATE TABLE IF NOT EXISTS tool_call_transitions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    reservation_key TEXT NOT NULL REFERENCES tool_calls(reservation_key),
    state TEXT NOT NULL
        CHECK (state IN (
            'proposed',
            'validated',
            'executing',
            'succeeded',
            'failed',
            'cancelled'
        )),
    result_json TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (reservation_key, state)
);

CREATE INDEX IF NOT EXISTS idx_tool_call_transitions_order
    ON tool_call_transitions(reservation_key, id ASC);
