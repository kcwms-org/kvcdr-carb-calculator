#!/bin/bash
set -euo pipefail

echo "Deploying carb-calculator stack..."

# Clone repo or pull latest
if [ ! -d /opt/carb-calculator ]; then
    echo "Cloning repository..."
    git clone https://github.com/kcwms-org/kvcdr-carb-calculator.git /opt/carb-calculator
else
    echo "Repository already exists, updating..."
    cd /opt/carb-calculator && git pull origin main
fi

cd /opt/carb-calculator

# Create .env if it doesn't exist
if [ ! -f .env ]; then
    echo "Creating .env template..."
    cat > .env << 'EOF'
# Required
ANTHROPIC_API_KEY=sk-...

# Optional
DEFAULT_ENGINE=claude
CACHE_TTL_SECS=86400
SERVER_PORT=3000

# Optional — Spaces (for presigned uploads)
SPACES_ACCESS_KEY=
SPACES_SECRET_KEY=
SPACES_REGION=nyc3
SPACES_BUCKET=s3-kvcdr
EOF
fi

echo ""
echo "Setup complete!"
echo ""
echo "Next steps:"
echo "  1. Edit /opt/carb-calculator/.env with your ANTHROPIC_API_KEY"
echo "  2. Start: docker compose --project-directory /opt/carb-calculator up --build -d"
echo "  3. View logs: docker compose --project-directory /opt/carb-calculator logs -f app"
echo ""
echo "Services (once started):"
echo "  - API:     http://localhost:3000"
echo "  - Grafana: http://localhost:3001 (admin / admin)"
echo "  - Loki:    http://localhost:3100"
echo ""
