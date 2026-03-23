# DigitalOcean App Platform Deployment

The app is deployed on DigitalOcean App Platform, connected directly to the Bitbucket repository. Any push to `main` triggers an automatic build and deploy — no CI/CD pipeline steps required.

## App Details

| Field | Value |
|---|---|
| App name | `kvcdr-carb-calculator` |
| App ID | `829b0364-60cb-4a28-8199-8b6184de882a` |
| URL | `https://kvcdr-carb-calculator-i38jg.ondigitalocean.app` |
| Region | Atlanta (`atl`) |
| Instance | Basic XXS — 1 shared vCPU, 512MB RAM ($5/mo) |
| Project | Contract |
| Source | Bitbucket `kevcoder1/kvcdr-carb-calculator`, branch `main` |
| Build | Docker — uses `Dockerfile` at repo root |

## Environment Variables

Set in App Platform under **Settings → Environment Variables**:

| Variable | Description | Secret |
|---|---|---|
| `ANTHROPIC_API_KEY` | Anthropic API key (`sk-ant-...`) | Yes |
| `DEFAULT_ENGINE` | AI engine to use (default: `claude`) | No |
| `SERVER_PORT` | Port the app listens on (default: `3000`) | No |
| `CACHE_TTL_SECS` | In-memory cache TTL in seconds (default: `86400`) | No |

> Note: `REDIS_URL` is not set — the app uses the in-memory Moka cache on App Platform.

## Auto-Deploy

App Platform watches the `main` branch on Bitbucket. Every push to `main` triggers a new build and deploy automatically. No manual steps required.

## Testing

```bash
# Health check
curl https://kvcdr-carb-calculator-i38jg.ondigitalocean.app/health

# Text only
curl -X POST https://kvcdr-carb-calculator-i38jg.ondigitalocean.app/analyze \
  -F "text=2 slices of white bread with peanut butter"

# Image only (keep under ~1MB)
curl -X POST https://kvcdr-carb-calculator-i38jg.ondigitalocean.app/analyze \
  -F "image=@/path/to/photo.jpg;type=image/jpeg"

# Image + text
curl -X POST https://kvcdr-carb-calculator-i38jg.ondigitalocean.app/analyze \
  -F "text=Waffle House" \
  -F "image=@/path/to/photo.jpg;type=image/jpeg"
```

> Image uploads must be under ~1MB. App Platform enforces a request body size limit. Resize large photos before uploading.

## Managing the App

```bash
# Check deployment status
doctl apps get 829b0364-60cb-4a28-8199-8b6184de882a

# View runtime logs
doctl apps logs 829b0364-60cb-4a28-8199-8b6184de882a api --type run

# View build logs
doctl apps logs 829b0364-60cb-4a28-8199-8b6184de882a api --type build

# Trigger a manual deploy
doctl apps create-deployment 829b0364-60cb-4a28-8199-8b6184de882a
```
