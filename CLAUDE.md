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

Deployed to a DigitalOcean droplet (NYC3, 1vCPU / 1GB RAM, $6/mo) running Docker Compose.

**Droplet:** `carb-calculator-prod` at `45.55.157.195`

**Services:**
- API: `:3000`
- Grafana: `:3001`
- Loki: `:3100`
- Promtail: ships logs to Loki

### Initial Setup (one-time)

SSH into the droplet and run the deployment script:

```bash
ssh root@45.55.157.195
curl -fsSL https://raw.githubusercontent.com/kcwms-org/kvcdr-carb-calculator/main/scripts/deploy.sh | bash
```

This clones the repo and writes a `.env` template. The droplet is assumed to have Docker and Docker Compose pre-installed.

After the script exits, fill in the API key, then start the stack:

```bash
nano /opt/carb-calculator/.env   # set ANTHROPIC_API_KEY
docker compose --project-directory /opt/carb-calculator up --build -d
```

### Deploying Changes

Push to `main` — the droplet will be set up with a deploy key and pull/restart automatically (optional cron job or webhook).

For manual deploys:

```bash
ssh root@45.55.157.195
cd /opt/carb-calculator
git pull origin main
docker compose up --build -d
```

### Domain

Point your DNS to `45.55.157.195` for `carb-calculator.kevcoder.com`. Optionally configure Cloudflare or another proxy for SSL.

### Env Vars

Create `.env` on the droplet before first deploy:

```
ANTHROPIC_API_KEY=...
DEFAULT_ENGINE=claude
CACHE_TTL_SECS=86400
SERVER_PORT=3000
SPACES_ACCESS_KEY=...
SPACES_SECRET_KEY=...
SPACES_REGION=nyc3
SPACES_BUCKET=s3-kvcdr
```

The deploy script creates this file; edit it post-deploy if needed.

## PR Workflow

After pushing a PR, if the user explicitly asks to monitor and auto-merge:
1. Start a background poll loop checking `gh pr checks <number> --watch`
2. Once all checks pass, merge with `gh pr merge <number> --squash --auto` (or `--merge` if squash is not appropriate)
3. Report the result

Only do this when explicitly asked — do not auto-merge by default.

## Logging (Loki + Grafana)

The app emits structured JSON logs to stdout. Locally, logs are shipped to Loki via Promtail and visualised in Grafana.

### Start the logging stack

```bash
docker compose up loki promtail grafana -d
```

Or start everything together:

```bash
docker compose up --build -d
```

### Grafana UI

`http://localhost:3001` — credentials: `admin` / `admin`

Navigate to **Explore → Loki** and query:

```
{job="carb-calculator"}
```

Filter by level:

```
{job="carb-calculator", level="error"}
```

### Loki readiness check

```bash
curl -s http://localhost:3100/ready   # → "ready"
```

### Query Loki directly

```bash
curl -G -s "http://localhost:3100/loki/api/v1/query_range" \
  --data-urlencode 'query={job="carb-calculator"}' \
  --data-urlencode "start=$(date -d '5 minutes ago' +%s)000000000" \
  --data-urlencode "end=$(date +%s)000000000" | jq '.data.result[].values[][1]'
```

### Notes

- `docker compose logs app` now emits raw JSON (one object per line) — this is expected
- Grafana runs on port **3001** to avoid clashing with the app on 3000
- Loki data is persisted in the `loki_data` Docker volume; Grafana state in `grafana_data`
- The Loki datasource is auto-provisioned from `grafana/provisioning/datasources/loki.yml` — no manual UI setup needed

## Adding a New AI Engine

1. Add `src/engines/<name>.rs` implementing the `AiEngine` trait
2. Register it in `build_engine()` in `src/engines/mod.rs`
