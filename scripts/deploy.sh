#!/bin/bash
set -euo pipefail

echo "Deploying carb-calculator stack..."

# Load system-wide environment variables (where secrets are stored on the droplet)
if [ -f /etc/environment ]; then
    set -a
    # shellcheck source=/dev/null
    source /etc/environment
    set +a
fi

# Clone repo or pull latest
if [ ! -d /opt/carb-calculator ]; then
    echo "Cloning repository..."
    git clone https://github.com/kcwms-org/kvcdr-carb-calculator.git /opt/carb-calculator
else
    echo "Repository already exists, updating..."
    cd /opt/carb-calculator && git pull origin main
fi

cd /opt/carb-calculator

# Write .env from environment variables (overwrites each deploy to pick up any changes)
echo "Writing .env from environment variables..."
cat > .env << EOF
ANTHROPIC_API_KEY=${ANTHROPIC_API_KEY:-}
DEFAULT_ENGINE=${DEFAULT_ENGINE:-claude}
CACHE_TTL_SECS=${CACHE_TTL_SECS:-86400}
SERVER_PORT=${SERVER_PORT:-3000}
SPACES_ACCESS_KEY=${SPACES_ACCESS_KEY:-}
SPACES_SECRET_KEY=${SPACES_SECRET_KEY:-}
SPACES_REGION=${SPACES_REGION:-nyc3}
SPACES_BUCKET=${SPACES_BUCKET:-s3-kvcdr}
EOF
chmod 600 .env

# Warn if required key is missing
if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
    echo "WARNING: ANTHROPIC_API_KEY is not set in /etc/environment — app will fail to start"
fi

echo ""
echo "Setup complete!"
echo ""
echo "Start the stack:"
echo "  docker compose --project-directory /opt/carb-calculator up --build -d"
echo ""
echo "View logs:"
echo "  docker compose --project-directory /opt/carb-calculator logs -f app"
echo ""
echo "Services:"
echo "  - API:     http://localhost:3000"
echo "  - Grafana: http://localhost:3001 (admin / admin)"
echo "  - Loki:    http://localhost:3100"
echo ""
