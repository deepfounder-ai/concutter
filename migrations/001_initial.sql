PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS rules (
    id TEXT PRIMARY KEY,
    pattern TEXT NOT NULL,
    replacement TEXT NOT NULL,
    layer TEXT NOT NULL CHECK(layer IN ('static', 'domain', 'learned')),
    domain TEXT,
    confidence REAL DEFAULT 0.0,
    samples INTEGER DEFAULT 0,
    enabled INTEGER DEFAULT 1,
    priority INTEGER DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS compression_stats (
    id TEXT PRIMARY KEY,
    request_id TEXT,
    provider TEXT,
    model TEXT,
    domain_detected TEXT,
    original_tokens INTEGER,
    compressed_tokens INTEGER,
    compression_ratio REAL,
    rules_applied TEXT,
    elapsed_us INTEGER,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS experiments (
    id TEXT PRIMARY KEY,
    rule_id TEXT REFERENCES rules(id),
    original_prompt TEXT,
    compressed_prompt TEXT,
    original_response TEXT,
    compressed_response TEXT,
    similarity_score REAL,
    status TEXT DEFAULT 'pending' CHECK(status IN ('pending', 'running', 'completed', 'failed')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    completed_at TEXT
);

CREATE TABLE IF NOT EXISTS rule_stats (
    rule_id TEXT PRIMARY KEY REFERENCES rules(id),
    times_applied INTEGER DEFAULT 0,
    total_tokens_saved INTEGER DEFAULT 0,
    avg_compression REAL DEFAULT 0.0,
    last_applied_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_rules_layer ON rules(layer);
CREATE INDEX IF NOT EXISTS idx_rules_domain ON rules(domain);
CREATE INDEX IF NOT EXISTS idx_rules_enabled ON rules(enabled);
CREATE INDEX IF NOT EXISTS idx_compression_stats_created ON compression_stats(created_at);
CREATE INDEX IF NOT EXISTS idx_experiments_status ON experiments(status);
CREATE INDEX IF NOT EXISTS idx_experiments_rule ON experiments(rule_id);
