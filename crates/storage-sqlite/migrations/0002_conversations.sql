-- Conversation domain authority: every conversation, message, run and event
-- is project-scoped through conversations.project_id -> projects.id.

CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects(id),
    title TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_project_created
    ON conversations(project_id, created_at ASC, id ASC);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    role TEXT NOT NULL
        CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation_created
    ON messages(conversation_id, created_at ASC, id ASC);

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversations(id),
    status TEXT NOT NULL
        CHECK (status IN (
            'pending',
            'running',
            'cancelling',
            'completed',
            'failed',
            'cancelled',
            'interrupted'
        )),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_runs_conversation_created
    ON runs(conversation_id, created_at ASC, id ASC);

CREATE INDEX IF NOT EXISTS idx_runs_status
    ON runs(status);

CREATE TABLE IF NOT EXISTS run_events (
    event_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(id),
    sequence INTEGER NOT NULL,
    kind TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    UNIQUE (run_id, sequence)
);

CREATE INDEX IF NOT EXISTS idx_run_events_run_sequence
    ON run_events(run_id, sequence ASC);
