CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    root_path TEXT NOT NULL,
    canonical_root_path TEXT NOT NULL UNIQUE,
    trust_state TEXT NOT NULL DEFAULT 'untrusted'
        CHECK (trust_state IN ('untrusted', 'trusted')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
