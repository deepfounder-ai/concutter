# Concutter (sqz)

Prompt compression proxy for OpenAI and Anthropic APIs. Reduces token usage by applying rule-based text compression with <1ms latency overhead.

## How it works

```
Client ──► sqz proxy ──► OpenAI / Anthropic
           │
           ├─ Compress user & system messages
           ├─ Protect code blocks, JSON, inline code
           ├─ Forward all other fields as-is
           └─ Stream responses back untouched
```

sqz sits between your application and the LLM API. It compresses text in `user` and `system` messages using an Aho-Corasick automaton built from three rule layers, then forwards the request to the upstream provider. Responses (including SSE streams) pass through unmodified.

## Supported endpoints

| Route | Method | Description |
|-------|--------|-------------|
| `/v1/chat/completions` | POST | OpenAI Chat Completions (streaming + non-streaming) |
| `/v1/messages` | POST | Anthropic Messages (streaming + non-streaming) |
| `/health` | GET | Health check |
| `/admin/rules` | GET | List compression rules (pagination, filtering) |
| `/admin/rules` | POST | Create a learned rule |
| `/admin/rules/{id}` | PUT | Update a rule |
| `/admin/rules/{id}` | DELETE | Delete a rule |
| `/admin/stats` | GET | Overall compression statistics |
| `/admin/stats/compression` | GET | Per-request compression stats |
| `/admin/reload` | POST | Rebuild compressor from current rules |
| `/admin/experiments` | GET | List A/B experiments |

## Quick start

### Docker (recommended)

```bash
docker build -t sqz .
docker run -p 8080:8080 sqz
```

Or with docker compose:

```bash
docker compose up
```

### From source

```bash
cargo build --release
./target/release/sqz --config concutter.toml
```

### Verify

```bash
curl http://localhost:8080/health
# {"status":"ok"}
```

## Usage

sqz is a drop-in proxy. Point your client at sqz instead of the provider API and pass your API key as usual:

**OpenAI:**
```bash
curl http://localhost:8080/v1/chat/completions \
  -H "Authorization: Bearer sk-..." \
  -H "Content-Type: application/json" \
  -d '{
    "model": "gpt-4o",
    "messages": [{"role": "user", "content": "Explain the concept of recursion"}]
  }'
```

**Anthropic:**
```bash
curl http://localhost:8080/v1/messages \
  -H "x-api-key: sk-ant-..." \
  -H "anthropic-version: 2023-06-01" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "claude-sonnet-4-5-20250929",
    "max_tokens": 1024,
    "messages": [{"role": "user", "content": "Explain the concept of recursion"}]
  }'
```

API keys are passed through to the upstream provider — sqz never stores them.

All request fields (`temperature`, `top_p`, `tools`, `response_format`, etc.) are forwarded as-is via `serde(flatten)`.

## Configuration

### Config file

Copy the example and edit:

```bash
cp concutter.example.toml concutter.toml
```

```toml
[server]
host = "127.0.0.1"
port = 8080
request_timeout = 120

[upstream]
openai_base_url = "https://api.openai.com"
anthropic_base_url = "https://api.anthropic.com"

[database]
path = "concutter.db"

[compression]
enabled = true
confidence_threshold = 0.8
min_samples = 10
default_language = "en"
languages = ["en"]

[compression.layers]
static_enabled = true    # Stopword rules
domain_enabled = true    # Domain-specific TOML rules
learned_enabled = true   # Rules from SQLite with confidence scoring

[shadow]
enabled = false          # A/B testing mode
sample_rate = 0.1
embedding_model = "text-embedding-3-small"

[logging]
level = "info"           # trace, debug, info, warn, error
json = false

[admin]
enabled = true
# api_key = "your-secret-admin-key"

[reload]
interval = 300           # Auto-reload compressor (seconds, 0 = disabled)
```

### CLI overrides

```
sqz --config concutter.toml --host 0.0.0.0 --port 8080 --db /data/concutter.db --log-level debug
```

### Environment variables

Environment variables override config file values (but CLI flags take priority):

| Variable | Config field | Example |
|----------|-------------|---------|
| `SQZ_HOST` | server.host | `0.0.0.0` |
| `SQZ_PORT` | server.port | `8085` |
| `SQZ_DB_PATH` | database.path | `/data/concutter.db` |
| `SQZ_OPENAI_BASE_URL` | upstream.openai_base_url | `https://api.openai.com` |
| `SQZ_ANTHROPIC_BASE_URL` | upstream.anthropic_base_url | `https://api.anthropic.com` |
| `SQZ_LOG_LEVEL` | logging.level | `debug` |
| `SQZ_COMPRESSION_ENABLED` | compression.enabled | `true` |

Priority: **defaults < TOML < env vars < CLI flags**

## Compression engine

### Three rule layers

1. **Static** — Stopword lists per language (`rules/stopwords/en.txt`, `ru.txt`). Common filler words removed.
2. **Domain** — TOML configs per domain (`rules/domains/code.toml`, `legal.toml`, etc.). Domain-specific abbreviations and replacements.
3. **Learned** — Rules stored in SQLite with confidence scoring. Created via admin API, activated when confidence >= threshold and samples >= min_samples.

### Protected regions

The compressor never modifies text inside:
- Code fences (` ``` `)
- Inline code (`` ` ``)
- JSON blocks
- Domain-specific protected terms

### How compression works

1. Count original tokens (tiktoken)
2. Auto-detect domain or use hint
3. Identify protected regions
4. Find pattern matches via Aho-Corasick (case-insensitive)
5. Check word boundaries
6. Remove overlapping matches (greedy, longest first)
7. Apply replacements
8. Collapse whitespace
9. Count compressed tokens, record statistics

Only `user` and `system` messages are compressed. `assistant`, `tool_use`, and `tool_result` messages pass through unmodified.

## Admin API

### Rules CRUD

```bash
# List rules
curl http://localhost:8080/admin/rules?limit=50&offset=0&layer=learned

# Create rule
curl -X POST http://localhost:8080/admin/rules \
  -H "Content-Type: application/json" \
  -d '{"pattern": "in order to", "replacement": "to", "layer": "learned"}'

# Update rule
curl -X PUT http://localhost:8080/admin/rules/{id} \
  -H "Content-Type: application/json" \
  -d '{"enabled": false}'

# Delete rule
curl -X DELETE http://localhost:8080/admin/rules/{id}
```

### Statistics

```bash
# Overall stats
curl http://localhost:8080/admin/stats
# {"total_requests":142,"total_tokens_saved":8430,"avg_compression_ratio":0.87,...}

# Per-request compression stats
curl http://localhost:8080/admin/stats/compression?limit=10
```

### Reload compressor

After creating/updating rules, rebuild the in-memory automaton:

```bash
curl -X POST http://localhost:8080/admin/reload
# {"success":true,"rules_count":156,"elapsed_ms":2}
```

## Docker deployment

### Dockerfile

Multi-stage build: `rust:1.85-bookworm` (build) + `debian:bookworm-slim` (runtime, ~80MB).

### Environment configuration

```yaml
# docker-compose.yml
services:
  sqz:
    build: .
    ports:
      - "8080:8080"
    volumes:
      - sqz-data:/data
    environment:
      SQZ_PORT: "8085"
      SQZ_OPENAI_BASE_URL: "https://api.openai.com"
      SQZ_ANTHROPIC_BASE_URL: "https://api.anthropic.com"
      SQZ_LOG_LEVEL: "info"

volumes:
  sqz-data:  # Persists SQLite database
```

### Easypanel

1. Connect GitHub repo
2. Set environment variables (`SQZ_PORT`, etc.)
3. Configure domain → container port matching `SQZ_PORT`
4. Deploy

## Architecture

```
concutter/
├── crates/
│   ├── sqz-core/      # Compression engine (Aho-Corasick, 3 layers, token counting)
│   ├── sqz-store/     # SQLite persistence (tokio-rusqlite)
│   ├── sqz-proxy/     # axum 0.8 HTTP proxy (OpenAI + Anthropic + Admin API)
│   └── sqz-bin/       # CLI entrypoint, config loading
├── migrations/        # SQLite migrations
├── rules/
│   ├── stopwords/     # en.txt, ru.txt
│   └── domains/       # code.toml, legal.toml, medical.toml, finance.toml
├── concutter.example.toml
├── Dockerfile
└── docker-compose.yml
```

### Key design decisions

- **Aho-Corasick automaton in RAM** — All rules compiled into a single automaton for O(n) pattern matching with <1ms latency
- **`Arc<RwLock<Compressor>>`** — Shared compressor state, read-locked per request, write-locked only on reload
- **`serde(flatten)`** — All request types capture unknown fields, ensuring forward compatibility with new API parameters
- **Fire-and-forget stats** — Compression statistics recorded asynchronously, never blocking the response
- **SSE passthrough** — Streaming responses forwarded byte-for-byte without buffering
- **Credential-agnostic** — API keys passed through from client headers, never stored

## License

MIT
