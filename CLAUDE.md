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

## Deployment

The app is deployed to a Digital Ocean Droplet (`143.244.174.42`) via Bitbucket Pipelines. The pipeline builds a Docker image, pushes it to Digital Ocean Container Registry (DOCR), then SSHes into the droplet to pull and run it.

### Bitbucket Pipeline Repo Variables

Set these in **Repository Settings → Pipelines → Repository variables**:

| Variable | Description | Secured |
|---|---|---|
| `DO_API_TOKEN` | Digital Ocean API token — used to authenticate with DOCR | Yes |
| `SSH_PRIVATE_KEY` | Contents of `~/.ssh/id_ed25519` — used to SSH into the droplet | Yes |
| `DROPLET_IP` | Droplet IP address (`143.244.174.42`) | No |
| `DROPLET_HOST_KEY` | Output of `ssh-keyscan -H <droplet-ip>` — prevents MITM on SSH | Yes |

### DOCR

Registry: `registry.digitalocean.com/kvcdr-registry`
Image: `registry.digitalocean.com/kvcdr-registry/kvcdr-carb-calculator:latest`

### One-Time Droplet Setup

```bash
sudo mkdir -p /opt/kvcdr-carb-calculator
sudo chown kevin:kevin /opt/kvcdr-carb-calculator
cd /opt/kvcdr-carb-calculator
git clone git@bitbucket.org:kevcoder1/kvcdr-carb-calculator.git .
cp .env.example .env
# edit .env with ANTHROPIC_API_KEY and other required values
```

## Adding a New AI Engine

1. Add `src/engines/<name>.rs` implementing the `AiEngine` trait
2. Register it in `build_engine()` in `src/engines/mod.rs`
