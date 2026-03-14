# CLAUDE.md

This file provides guidance to Claude Code when working in this repository.

## Project

`kvcdr-carb-calculator` — Rust/Axum HTTP API that estimates carbohydrates per food item using Claude vision AI. Supports multipart image upload, text-only, or combined input.

## Commands

```bash
cargo build         # Build
cargo run           # Start server (requires .env with ANTHROPIC_API_KEY)
cargo test          # Run tests
cargo clippy        # Lint
```

## Workflow Rules

- All features and bug fixes must be developed on a new branch — never commit directly to `main`.
- Commit as soon as a change builds successfully.
- Push to the remote (`origin`) as soon as all commits on the branch build and all tests (if any) pass.

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
- `image` — optional file upload
- `text` — optional text description
- `engine` — optional engine name (default: `claude`)

Returns `AnalyzeResponse` JSON with per-item carb breakdown, total, engine used, and cache hit flag.

## Environment Variables

See `.env.example`:
```
ANTHROPIC_API_KEY=...
DEFAULT_ENGINE=claude
CACHE_TTL_SECS=86400
SERVER_PORT=3000
```

## Adding a New AI Engine

1. Add `src/engines/<name>.rs` implementing the `AiEngine` trait
2. Register it in `build_engine()` in `src/engines/mod.rs`
