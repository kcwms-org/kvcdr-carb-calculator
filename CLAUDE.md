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
├── spaces.rs             # SpacesClient — presigned PUT + delete via AWS S3-compatible API
└── routes/
    ├── analyze.rs        # POST /analyze multipart handler
    └── presign.rs        # GET /presign, DELETE /upload/{key}
```

## Endpoints

`POST /analyze` — multipart/form-data fields:
- `image` — optional file upload (small images only; DO App Platform enforces a ~1 MB ingress limit)
- `image_url` — optional public URL of a pre-uploaded image (e.g. DigitalOcean Spaces); **preferred for phone camera photos** to bypass platform upload limits. Claude receives the URL directly via its `url` image source type.
- `text` — optional text description
- `engine` — optional engine name (default: `claude`)

At least one of `image`, `image_url`, or `text` is required.

Returns `AnalyzeResponse` JSON with per-item carb breakdown, total, engine used, and cache hit flag.

`GET /presign` — returns a presigned PUT URL for the client to upload directly to DO Spaces (`s3-kvcdr`, `nyc3`). Response:
```json
{
  "upload_url": "https://s3-kvcdr.nyc3.digitaloceanspaces.com/tmp/<uuid>?...",
  "image_url": "https://s3-kvcdr.nyc3.digitaloceanspaces.com/tmp/<uuid>",
  "key": "tmp/<uuid>",
  "required_headers": { "x-amz-acl": "public-read" }
}
```
The client **must** send all `required_headers` with the PUT or DO Spaces will reject it. Requires Spaces env vars.

`DELETE /upload/{key}` — deletes a temporary Spaces object after analysis. Requires Spaces env vars.

> **Recommended flow for large images (phone camera photos):**
> 1. `GET /presign` → get `upload_url`, `image_url`, `key`, `required_headers`
> 2. `PUT {upload_url}` with image bytes and all `required_headers` (e.g. `x-amz-acl: public-read`)
> 3. `POST /analyze` with `image_url`
> 4. `DELETE /upload/{key}` to clean up

## Environment Variables

See `.env.example`:
```
ANTHROPIC_API_KEY=...
DEFAULT_ENGINE=claude
CACHE_TTL_SECS=86400
SERVER_PORT=3000

# Optional — enables GET /presign and DELETE /upload/{key}
SPACES_ACCESS_KEY=...
SPACES_SECRET_KEY=...
SPACES_REGION=nyc3
SPACES_BUCKET=s3-kvcdr
```

## Deployment

The app is deployed via **DigitalOcean App Platform** (ATL region, $5/mo shared CPU).

- Pushes to `main` trigger an automatic rebuild and redeploy — no pipeline step needed
- Custom domain: `carb-calculator.kevcoder.com`
- Env vars (`ANTHROPIC_API_KEY`, `DEFAULT_ENGINE`, `CACHE_TTL_SECS`, `SERVER_PORT`, `SPACES_ACCESS_KEY`, `SPACES_SECRET_KEY`) are configured in the App Platform dashboard

## PR Workflow

After pushing a PR, if the user explicitly asks to monitor and auto-merge:
1. Start a background poll loop checking `gh pr checks <number> --watch`
2. Once all checks pass, merge with `gh pr merge <number> --squash --auto` (or `--merge` if squash is not appropriate)
3. Report the result

Only do this when explicitly asked — do not auto-merge by default.

## Adding a New AI Engine

1. Add `src/engines/<name>.rs` implementing the `AiEngine` trait
2. Register it in `build_engine()` in `src/engines/mod.rs`
