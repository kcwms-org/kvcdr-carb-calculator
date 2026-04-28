# CLAUDE.md

This file provides guidance to Claude Code when working in this repository.

## Project

`kvcdr-carb-calculator` — Rust/Axum HTTP API that estimates carbohydrates per food item using Claude vision AI. Accepts a multipart image upload, a text description, or both.

## Commands

```bash
cargo build         # Build
cargo run           # Start server (requires .env with ANTHROPIC_API_KEY)
cargo test          # Run tests
cargo clippy        # Lint
```

## Architecture

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
    └── analyze.rs        # POST /analyze multipart handler
```

## Endpoints

`POST /analyze` — multipart/form-data fields:
- `image` — optional image file (JPEG, PNG, GIF, WebP). DO App Platform enforces a ~1 MB ingress limit, so clients **must resize/compress phone camera photos before upload** (target: ≤1 MB, e.g. 1280px longest edge at JPEG quality ~80).
- `text` — optional text description
- `datetime` — optional RFC 3339 meal timestamp; defaults to server time, rejected if in the future

At least one of `image` or `text` is required.

Returns `AnalyzeResponse` JSON with per-item carb breakdown, total, engine used, cache hit flag, datetime, and the uploaded image (base64) echoed back.

`GET /health` — returns `OK`. Used by App Platform health check.

`GET /swagger-ui` and `GET /api-docs/openapi.json` — interactive OpenAPI docs.

## Environment Variables

See `.env.example`:
```
ANTHROPIC_API_KEY=...
DEFAULT_ENGINE=claude
CACHE_TTL_SECS=86400
SERVER_PORT=3000
AI_EXTRACTION_MODEL=claude-haiku-4-5-20251001
AI_REASONING_MODEL=claude-sonnet-4-6
REDIS_URL=redis://localhost:6379   # optional
```

## Deployment

Deployed to **DigitalOcean App Platform** (app id `10008a08-078b-420d-81a6-2ec817254292`, region `atl`). The platform builds from the repo's `Dockerfile`, sets env vars from the app spec, and deploys on push to `main`.

Domain: `carb-calculator.kevcoder.com`
Default ingress: `https://kvcdr-carb-calculator-y9daf.ondigitalocean.app`

### Useful `doctl` commands

```bash
# Live app spec (env vars, build config, health check)
doctl apps spec get 10008a08-078b-420d-81a6-2ec817254292

# Deployment history / status
doctl apps list-deployments 10008a08-078b-420d-81a6-2ec817254292

# Tail runtime logs
doctl apps logs 10008a08-078b-420d-81a6-2ec817254292 --type run --follow

# Trigger a redeploy without a code change
doctl apps create-deployment 10008a08-078b-420d-81a6-2ec817254292
```

### Ingress limit

App Platform caps request bodies at roughly 1 MB. The API accepts up to 20 MB at the Axum layer for local-dev convenience, but in production large bodies are rejected at the platform edge before they reach the app. Clients are responsible for resizing images before upload.

## PR Workflow

After pushing a PR, if the user explicitly asks to monitor and auto-merge:
1. Start a background poll loop checking `gh pr checks <number> --watch`
2. Once all checks pass, merge with `gh pr merge <number> --squash --auto` (or `--merge` if squash is not appropriate)
3. Report the result

Only do this when explicitly asked — do not auto-merge by default.

## Adding a New AI Engine

1. Add `src/engines/<name>.rs` implementing the `AiEngine` trait (and `ExtractionEngine` if it should also handle phase-1 extraction)
2. Register it in `build_engine()` / `build_extraction_engine()` in `src/engines/mod.rs`
