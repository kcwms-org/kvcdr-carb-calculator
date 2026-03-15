# kvcdr-carb-calculator

A Rust HTTP API that estimates carbohydrates per food item in a meal. Submit a food photo, a text description, or both — the API delegates to an AI vision model (Claude by default) and returns a structured JSON breakdown with per-item carb estimates and a running total.

Results are cached (moka in-process + optional Redis) so repeated requests for the same meal are served instantly.

---

## Architecture

```
POST /analyze (multipart/form-data)
        │
        ▼
  analyze_handler
  ├── Parse multipart fields (image, text)
  ├── Build cache key
  │     ├── text: normalised (trim + lowercase) → SHA-256
  │     └── image: perceptual hash (Gradient pHash, 8×8)
  │           └── near-identical images → same bucket (Hamming ≤ 10)
  ├── Cache lookup  ──hit──▶  return cached AnalyzeResponse (cached: true)
  │        │
  │       miss
  │        ▼
  ├── AiEngine::analyze(AnalysisInput)
  │     └── ClaudeEngine → Anthropic API (claude-sonnet-4-5, vision)
  ├── Cache write (moka + Redis)
  └── return AnalyzeResponse (cached: false)
```

### Module layout

```
src/
├── main.rs               # Axum router, server startup
├── config.rs             # Config loaded from env vars (dotenvy)
├── error.rs              # AppError (thiserror) + IntoResponse impl
├── models/mod.rs         # FoodItem, AnalyzeResponse
├── engines/
│   ├── mod.rs            # AiEngine trait + build_engine factory
│   └── claude.rs         # ClaudeEngine — Anthropic API with vision support
├── cache/mod.rs          # Two-layer cache: moka (L1) + Redis (L2)
└── routes/
    ├── analyze.rs        # POST /analyze handler
    └── tests.rs          # Integration + unit tests
```

### Key types

```rust
// Input to any AI engine
pub struct AnalysisInput {
    pub image_bytes: Option<Vec<u8>>,
    pub image_mime:  Option<String>,   // e.g. "image/jpeg"
    pub text:        Option<String>,
}

// One food item returned by the engine
pub struct FoodItem {
    pub name:         String,
    pub carbs_grams:  f32,
    pub confidence:   Option<String>,  // "high" | "medium" | "low"
    pub notes:        Option<String>,
}

// Top-level response
pub struct AnalyzeResponse {
    pub items:             Vec<FoodItem>,
    pub total_carbs_grams: f32,
    pub engine_used:       String,
    pub cached:            bool,
}
```

### Caching strategy

| Layer | Backend | Scope | Notes |
|-------|---------|-------|-------|
| L1 | `moka::future::Cache` | in-process | max 1 000 entries, TTL-evicted |
| L2 | Redis | shared / multi-instance | Redis hit backfills L1 |

Cache key = SHA-256 of `engine_name + normalised_text + perceptual_image_hash`.
Images within Hamming distance ≤ 10 of each other share the same cache bucket.

### Adding a new AI engine

1. Create `src/engines/<name>.rs` implementing `AiEngine`:
   ```rust
   #[async_trait]
   impl AiEngine for MyEngine {
       fn name(&self) -> &str { "myengine" }
       async fn analyze(&self, input: AnalysisInput) -> Result<Vec<FoodItem>, AppError> { … }
   }
   ```
2. Register it in `build_engine()` in `src/engines/mod.rs`.
3. Set `DEFAULT_ENGINE=myengine` in `.env`.

---

## Building and running locally

### Prerequisites

- Rust toolchain (`rustup` — stable channel)
- Docker + Docker Compose (for optional Redis)
- An [Anthropic API key](https://console.anthropic.com/)

### 1 — Clone and configure

```bash
git clone git@bitbucket.org:kevcoder1/kvcdr-carb-calculator.git
cd kvcdr-carb-calculator
cp .env.example .env
# Edit .env and set ANTHROPIC_API_KEY=sk-ant-...
```

### 2 — Start Redis (optional but recommended)

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

The service is a single stateless binary. Typical deployment steps:

```bash
cargo build --release
# Binary: target/release/kvcdr-carb-calculator

# Set required environment variables, then run:
ANTHROPIC_API_KEY=sk-ant-... \
DEFAULT_ENGINE=claude \
CACHE_TTL_SECS=86400 \
SERVER_PORT=3000 \
REDIS_URL=redis://your-redis-host:6379 \
./target/release/kvcdr-carb-calculator
```

For containerised deployments, build a Docker image with a `rust:slim` base and copy the release binary.

---

## Environment variables

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `ANTHROPIC_API_KEY` | yes | — | Anthropic API key |
| `DEFAULT_ENGINE` | no | `claude` | AI engine to use |
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
| `image` | file | Food photo (JPEG, PNG, GIF, WebP) |
| `text` | string | Text description or supplementary context |

#### Response — `200 OK`

```json
{
  "items": [
    {
      "name": "oatmeal",
      "carbs_grams": 27.0,
      "confidence": "high",
      "notes": null
    },
    {
      "name": "banana",
      "carbs_grams": 27.0,
      "confidence": "high",
      "notes": null
    }
  ],
  "total_carbs_grams": 54.0,
  "engine_used": "claude",
  "cached": false
}
```

#### Error responses

| Status | Condition |
|--------|-----------|
| `400 Bad Request` | Neither `image` nor `text` provided, or whitespace-only text |
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
# First request — cache miss
curl -s -X POST http://localhost:3000/analyze \
  -F "text=pasta with tomato sauce" | jq .cached
# false

# Second request — cache hit
curl -s -X POST http://localhost:3000/analyze \
  -F "text=pasta with tomato sauce" | jq .cached
# true
```

### Pretty-print with jq

```bash
curl -s -X POST http://localhost:3000/analyze \
  -F "text=chicken tikka masala with basmati rice" | jq .
```
