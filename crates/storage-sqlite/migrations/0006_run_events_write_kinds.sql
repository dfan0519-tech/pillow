-- C3-F: rebuild run_events so kind CHECK accepts write_proposal_requested /
-- write_proposal_resolved. SQLite cannot ALTER a CHECK in place.
-- Applied only when migrations.rs detects the live CHECK still rejects those
-- kinds. The batch is wrapped in BEGIN/COMMIT by migrations.rs so a mid-flight
-- failure rolls back rather than leaving a half-rebuilt table.

CREATE TABLE run_events__new (
    event_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id),
    sequence INTEGER NOT NULL
        CHECK (sequence >= 0),
    kind TEXT NOT NULL
        CHECK (kind IN (
            'run_started',
            'assistant_delta',
            'reasoning_summary_delta',
            'plan_updated',
            'tool_call_started',
            'tool_call_output',
            'tool_call_completed',
            'subagent_started',
            'subagent_progress',
            'subagent_completed',
            'artifact_produced',
            'write_proposal_requested',
            'write_proposal_resolved',
            'turn_completed',
            'turn_failed',
            'run_cancelled',
            'heartbeat'
        )),
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (run_id, sequence)
);

INSERT INTO run_events__new (
    event_id, run_id, sequence, kind, payload_json, created_at
)
SELECT event_id, run_id, sequence, kind, payload_json, created_at
FROM run_events;

DROP TABLE run_events;

ALTER TABLE run_events__new RENAME TO run_events;

CREATE INDEX IF NOT EXISTS idx_run_events_run_sequence
    ON run_events(run_id, sequence ASC);
