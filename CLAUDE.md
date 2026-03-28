# CLAUDE.md

This file provides guidance to Claude Code when working in this repository.

## Project

`kvcdr-carb-calculator` — Rust/Axum HTTP API that estimates carbohydrates per food item using Claude vision AI. Supports multipart image upload, pre-uploaded image URL (recommended for large files), text-only, or combined input.

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
├── models/mod.rs         # FoodItem, AnalyzeResponse
├── engines/
│   ├── mod.rs            # AiEngine trait + build_engine factory
│   └── claude.rs         # ClaudeEngine — calls Anthropic API with vision
├── cache/mod.rs          # Moka async cache wrapper (SHA-256 keyed, 24h TTL)
└── routes/
    └── analyze.rs        # POST /analyze multipart handler
```

## Endpoint

`POST /analyze` — multipart/form-data fields:
- `image` — optional file upload (small images only; DO App Platform enforces a ~1 MB ingress limit)
- `image_url` — optional public URL of a pre-uploaded image (e.g. DigitalOcean Spaces); **preferred for phone camera photos** to bypass platform upload limits. Claude receives the URL directly via its `url` image source type.
- `text` — optional text description
- `engine` — optional engine name (default: `claude`)

At least one of `image`, `image_url`, or `text` is required.

Returns `AnalyzeResponse` JSON with per-item carb breakdown, total, engine used, and cache hit flag.

> **Note on image upload size:** DO App Platform's ingress proxy limits request bodies to ~1 MB. For large images (phone camera photos are typically 3–10 MB), upload the image directly to DO Spaces from the client and pass the resulting public URL as `image_url`. The API will forward the URL to Claude — no large bytes traverse the platform.

## Environment Variables

See `.env.example`:
```
ANTHROPIC_API_KEY=...
DEFAULT_ENGINE=claude
CACHE_TTL_SECS=86400
SERVER_PORT=3000
```

## Deployment

The app is deployed via **DigitalOcean App Platform** (ATL region, $5/mo shared CPU).

- Pushes to `main` trigger an automatic rebuild and redeploy — no pipeline step needed
- Custom domain: `carb-calculator.kevcoder.com`
- Env vars (`ANTHROPIC_API_KEY`, `DEFAULT_ENGINE`, `CACHE_TTL_SECS`, `SERVER_PORT`) are configured in the App Platform dashboard

## Adding a New AI Engine

1. Add `src/engines/<name>.rs` implementing the `AiEngine` trait
2. Register it in `build_engine()` in `src/engines/mod.rs`
