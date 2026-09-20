# Deploy kvcdr-carb-calculator to a DigitalOcean Droplet

This guide deploys `kcwms-org/kvcdr-carb-calculator` (the Rust/Axum carb-estimation API) onto your existing DigitalOcean droplet — 1 vCPU, 1GB RAM, 25GB disk. Given the RAM ceiling, the approach here is a **native build** (Rust toolchain installed directly on the droplet, binary run under systemd) rather than Docker, with **nginx** in front as the reverse proxy and TLS terminator. This skips the full `docker-compose.yml` stack (Redis, Grafana, Loki, Promtail) — that stack alone would consume most of a 1GB box; the API runs fine without Redis (it falls back to in-process Moka caching only).

**Before you start, have on hand:**

- SSH access to the droplet (root or a key-based sudo user)
- The droplet's public IP address
- An Anthropic API key (for `ANTHROPIC_API_KEY`)
- Optionally, a domain name you can point at the droplet (for TLS via Let's Encrypt) — otherwise you'll reach the API over plain HTTP by IP

## Step 1 — Initial server setup

SSH in and do basic hardening plus a **swap file** — with only 1GB RAM, `cargo build --release` on this dependency set (tokio full, utoipa, redis, etc.) can spike past physical memory and get OOM-killed without it.

```bash
ssh root@<droplet-ip>

# Create a non-root sudo user
adduser deploy
usermod -aG sudo deploy
rsync --archive --chown=deploy:deploy ~/.ssh /home/deploy

# Add a 2GB swap file (critical on a 1GB droplet)
fallocate -l 2G /swapfile
chmod 600 /swapfile
mkswap /swapfile
swapon /swapfile
echo '/swapfile none swap sw 0 0' >> /etc/fstab

# Unattended security updates
apt update && apt install -y unattended-upgrades
dpkg-reconfigure --priority=low unattended-upgrades
```

From here on, log in as `deploy` (`ssh deploy@<droplet-ip>`) and use `sudo` rather than staying root.

## Step 2 — Install build dependencies and Rust

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libssl-dev ca-certificates git curl

# Install Rust via rustup (as the deploy user)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustc --version
cargo --version
```

The app pins `edition = "2021"` and builds against the stable toolchain — rustup's default `stable` channel is fine.

## Step 3 — Clone and build

```bash
sudo mkdir -p /opt/carb-calculator
sudo chown deploy:deploy /opt/carb-calculator
git clone https://github.com/kcwms-org/kvcdr-carb-calculator.git /opt/carb-calculator
cd /opt/carb-calculator

cargo build --release --locked
```

On a 1 vCPU / 1GB droplet this build is slow (expect 10–20+ minutes) and leans on the swap file from Step 1 — that's expected, just let it run. The release binary lands at `target/release/kvcdr-carb-calculator`.

If the build still gets OOM-killed even with swap, the reliable fallback is to build the release binary elsewhere (a CI runner, or `cargo build --release` on a larger machine using the same Rust edition) and `scp` the resulting binary plus the `prompts/` directory over, skipping compilation on the droplet entirely.

## Step 4 — Configure environment variables

```bash
cd /opt/carb-calculator
cat > .env << 'EOF'
ANTHROPIC_API_KEY=sk-ant-your-key-here
DEFAULT_ENGINE=claude
AI_EXTRACTION_MODEL=claude-haiku-4-5-20251001
AI_REASONING_MODEL=claude-sonnet-4-6
CACHE_TTL_SECS=86400
SERVER_PORT=3000
EOF

chmod 600 .env
```

`REDIS_URL` is deliberately left unset — the config (`src/config.rs`) treats it as optional and the app falls back to the in-process Moka cache only. Running Redis alongside the API on a 1GB droplet is possible but leaves little headroom; skip it unless you're seeing cache-miss cost problems.

## Step 5 — Run it as a systemd service

```bash
sudo useradd --no-create-home --shell /usr/sbin/nologin appuser
sudo chown -R appuser:appuser /opt/carb-calculator

sudo tee /etc/systemd/system/kvcdr-carb-calculator.service << 'EOF'
[Unit]
Description=kvcdr-carb-calculator API
After=network.target

[Service]
Type=simple
User=appuser
Group=appuser
WorkingDirectory=/opt/carb-calculator
EnvironmentFile=/opt/carb-calculator/.env
ExecStart=/opt/carb-calculator/target/release/kvcdr-carb-calculator
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
ProtectSystem=strict
ReadWritePaths=/opt/carb-calculator

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable --now kvcdr-carb-calculator
sudo systemctl status kvcdr-carb-calculator
```

The app reads `prompts/extraction.txt` and `prompts/reasoning.txt` relative to `WorkingDirectory`, so `ExecStart` and `WorkingDirectory` must both point at `/opt/carb-calculator`.

## Step 6 — Install nginx as a reverse proxy

```bash
sudo apt install -y nginx

sudo tee /etc/nginx/sites-available/carb-calculator << 'EOF'
server {
    listen 80;
    server_name your-domain.example.com;  # or the droplet's IP

    client_max_body_size 15M;  # food photo uploads

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
        proxy_read_timeout 120s;
    }
}
EOF

sudo ln -s /etc/nginx/sites-available/carb-calculator /etc/nginx/sites-enabled/
sudo rm -f /etc/nginx/sites-enabled/default
sudo nginx -t
sudo systemctl reload nginx
```

`client_max_body_size` is raised because the multipart analyze endpoint accepts food photos; adjust if your images run larger. `proxy_read_timeout` gives the Claude vision/reasoning round trip room to finish.

## Step 7 — DigitalOcean Cloud Firewall

Skip `ufw` here — use DigitalOcean's own Firewall product instead. It filters traffic before it reaches the droplet and stays in effect across redeploys or resizes without any on-server reconfiguration.

In the DigitalOcean control panel: **Networking → Firewalls → Create Firewall**. Add inbound rules for SSH (22), HTTP (80), and HTTPS (443), then attach the firewall to this droplet. That covers the same ports the `ufw` rules would have opened, enforced at the network edge instead.

## Step 8 — TLS with Let's Encrypt (optional, recommended)

Point your domain's A record at the droplet's public IP first, then:

```bash
sudo apt install -y certbot python3-certbot-nginx
sudo certbot --nginx -d your-domain.example.com
```

Certbot edits the nginx server block to redirect port 80 to 443 and installs the certificate; it also sets up a systemd timer for auto-renewal (`sudo systemctl status certbot.timer` to confirm). Skip this step if you're accessing the API by bare IP — you'll stay on HTTP.

## Step 9 — Verify

```bash
# Directly against the app
curl http://localhost:3000/

# Through nginx, by domain or IP
curl http://your-domain.example.com/
# or, with TLS: curl https://your-domain.example.com/

# Service health and logs
sudo systemctl status kvcdr-carb-calculator
sudo journalctl -u kvcdr-carb-calculator -f
```

Swagger UI (added via `utoipa-swagger-ui`) should be reachable at `/swagger-ui` on the same host if you want to exercise the `analyze` endpoint from a browser.

## Step 10 — Redeploying updates

```bash
cd /opt/carb-calculator
git pull origin main
cargo build --release --locked
sudo systemctl restart kvcdr-carb-calculator
```

As a one-line helper, save this as `~/update.sh`:

```bash
#!/bin/bash
set -euo pipefail
cd /opt/carb-calculator
git pull origin main
cargo build --release --locked
sudo systemctl restart kvcdr-carb-calculator
sudo systemctl status kvcdr-carb-calculator --no-pager
```

The repo's own `scripts/deploy.sh` targets a Docker-based flow (writes `.env` from `/etc/environment`, expects `docker compose`) — it doesn't apply to this native setup, so use the commands above instead.

## Notes and troubleshooting

- **1GB RAM is the binding constraint.** The swap file in Step 1 gets you through the build; leave it in place permanently — the running binary itself is lightweight, but `cargo build` on updates will need it again.
- **Don't run the full `docker-compose.yml` stack here.** Redis, Grafana, Loki, and Promtail together want well over 1GB; this guide intentionally runs only the API binary plus nginx. If you want observability later, consider a separate droplet for it or a hosted log/metrics service instead.
- **`.env` permissions:** keep it at `chmod 600`, owned by `appuser`, and never commit it — it holds `ANTHROPIC_API_KEY`.
- **Build failing under memory pressure even with swap:** build the release binary on a separate machine (or a CI job) with the same Rust edition and `scp` `target/release/kvcdr-carb-calculator` plus `prompts/` to `/opt/carb-calculator` on the droplet, then skip straight to Step 5.
- **25GB disk:** plenty for the OS, toolchain, and binary; the main thing to watch is `target/` build artifacts accumulating across rebuilds — `cargo clean` if disk gets tight.
- **Port already in use / service won't start:** check `sudo ss -tlnp | grep 3000` and `sudo journalctl -u kvcdr-carb-calculator -n 50` for the actual error.
