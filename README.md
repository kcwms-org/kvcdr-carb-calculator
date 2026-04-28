# kvcdr-carb-calculator

A Rust HTTP API that estimates carbohydrates per food item in a meal. Submit a food photo, a text description, or both — the API delegates to an AI vision model (Claude by default) and returns a structured JSON breakdown with per-item carb estimates and a running total.

Results are cached (moka in-process + optional Redis) so repeated requests for the same meal are served instantly.

---

## Architecture

```
POST /analyze (multipart/form-data: image, text, datetime)
        │
        ▼
  analyze_handler
  ├── Parse multipart fields
  ├── Phase 1 — extraction engine identifies food items
  ├── Build cache key from extraction output
  ├── Cache lookup ── hit ──▶ return cached AnalyzeResponse (cached: true)
  │       │
  │      miss
  │       ▼
  ├── Phase 2 — reasoning engine estimates carbs per item
  ├── Cache write (moka + optional Redis)
  └── return AnalyzeResponse (cached: false)
```

### Module layout

```
src/
├── main.rs               # Axum router, server startup
├── config.rs             # Config loaded from env vars (dotenvy)
├── error.rs              # AppError (thiserror) + IntoResponse impl
├── models/mod.rs         # FoodItem, AnalyzeResponse, ImageData
├── engines/
│   ├── mod.rs            # AiEngine + ExtractionEngine traits + factory
│   └── claude.rs         # Claude reasoning + extraction engines
├── cache/mod.rs          # Moka L1 + optional Redis L2 cache
└── routes/
    ├── analyze.rs        # POST /analyze handler
    └── tests.rs          # Integration + unit tests
```

### Caching strategy

| Layer | Backend | Scope | Notes |
|-------|---------|-------|-------|
| L1 | `moka::future::Cache` | in-process | TTL-evicted |
| L2 | Redis | shared / multi-instance | Redis hit backfills L1 |

Cache key is derived from the reasoning model name, the extraction-prompt version, and a normalised, order-independent hash of the extracted item list.

### Adding a new AI engine

1. Create `src/engines/<name>.rs` implementing `AiEngine` (and `ExtractionEngine` if applicable).
2. Register it in `build_engine()` / `build_extraction_engine()` in `src/engines/mod.rs`.
3. Set `DEFAULT_ENGINE=<name>` in `.env`.

---

## Building and running locally

### Prerequisites

- Rust toolchain (`rustup` — stable channel)
- Docker + Docker Compose (for optional Redis)
- An [Anthropic API key](https://console.anthropic.com/)

### 1 — Configure

```bash
cp .env.example .env
# Edit .env and set ANTHROPIC_API_KEY=sk-ant-...
```

### 2 — Start Redis (optional)

```bash
docker compose up -d
```

### 3 — Build and run

```bash
cargo build
cargo run
# Server listening on 0.0.0.0:3000
```

### 4 — Run tests

```bash
cargo test
```

Tests are fully offline — they use mock engines and do not require an API key or Redis.

---

## Deployment

The service is deployed to **DigitalOcean App Platform**, which builds from the repo's `Dockerfile` and redeploys on every push to `main`.

```bash
doctl apps spec get 10008a08-078b-420d-81a6-2ec817254292
doctl apps logs 10008a08-078b-420d-81a6-2ec817254292 --type run --follow
```

App Platform caps request bodies at ~1 MB. Clients must resize images before upload (target ≤1 MB).

---

## Environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ANTHROPIC_API_KEY` | yes | — | Anthropic API key |
| `DEFAULT_ENGINE` | no | `claude` | AI engine to use |
| `AI_EXTRACTION_MODEL` | no | `claude-haiku-4-5-20251001` | Phase-1 extraction model |
| `AI_REASONING_MODEL` | no | `claude-sonnet-4-6` | Phase-2 reasoning model |
| `CACHE_TTL_SECS` | no | `86400` | Cache entry lifetime (seconds) |
| `SERVER_PORT` | no | `3000` | TCP port to listen on |
| `REDIS_URL` | no | — | Redis connection URL; moka-only if unset |

See `.env.example` for a ready-to-copy template.

---

## API reference

### `POST /analyze`

Accepts `multipart/form-data`. At least one of `image` or `text` must be provided.

| Field | Type | Description |
|-------|------|-------------|
| `image` | file | Food photo (JPEG, PNG, GIF, WebP). Resize before upload — production caps bodies at ~1 MB. |
| `text` | string | Text description or supplementary context |
| `datetime` | string | Optional RFC 3339 meal timestamp; rejected if in the future |

#### Response — `200 OK`

```json
{
  "items": [
    { "name": "oatmeal", "carbs_grams": 27.0, "confidence": "high", "notes": null },
    { "name": "banana",  "carbs_grams": 27.0, "confidence": "high", "notes": null }
  ],
  "total_carbs_grams": 54.0,
  "engine_used": "claude-sonnet-4-6",
  "cached": false,
  "datetime": "2026-04-08T12:00:00Z",
  "images": []
}
```

#### Error responses

| Status | Condition |
|--------|-----------|
| `400 Bad Request` | Neither `image` nor `text` provided, or whitespace-only text, or invalid/future datetime |
| `422 Unprocessable Entity` | AI engine returned unparseable JSON |
| `502 Bad Gateway` | AI engine call failed |
| `500 Internal Server Error` | Unexpected server error |

Error body: `{ "error": "<message>" }`

---

## curl examples

### Text-only request

```bash
curl -s -X POST http://localhost:3000/analyze \
  -F "text=a bowl of oatmeal with a sliced banana"
```

### Image upload

```bash
curl -s -X POST http://localhost:3000/analyze \
  -F "image=@/path/to/meal.jpg"
```

### Image with supplementary text

```bash
curl -s -X POST http://localhost:3000/analyze \
  -F "image=@/path/to/meal.jpg" \
  -F "text=large serving, extra sauce"
```

### Verify caching — second identical request returns `cached: true`

```bash
curl -s -X POST http://localhost:3000/analyze -F "text=pasta with tomato sauce" | jq .cached
# false
curl -s -X POST http://localhost:3000/analyze -F "text=pasta with tomato sauce" | jq .cached
# true
```
