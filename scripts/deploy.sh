#!/bin/bash
set -euo pipefail

echo "🚀 Deploying carb-calculator stack..."

# Update system
apt-get update && apt-get upgrade -y

# Install Docker
if ! command -v docker &> /dev/null; then
    echo "📦 Installing Docker..."
    curl -fsSL https://get.docker.com -o /tmp/get-docker.sh
    bash /tmp/get-docker.sh
    rm /tmp/get-docker.sh
fi

if ! command -v docker-compose &> /dev/null; then
    echo "📦 Installing Docker Compose..."
    curl -fsSL https://github.com/docker/compose/releases/download/v2.20.0/docker-compose-linux-x86_64 -o /usr/local/bin/docker-compose
    chmod +x /usr/local/bin/docker-compose
fi

# Clone repo
if [ ! -d /opt/carb-calculator ]; then
    echo "📂 Cloning repository..."
    git clone https://github.com/kvcdr/carb-calculator.git /opt/carb-calculator
else
    echo "✅ Repository already exists, updating..."
    cd /opt/carb-calculator && git pull origin main
fi

cd /opt/carb-calculator

# Create .env if it doesn't exist
if [ ! -f .env ]; then
    echo "📝 Creating .env template..."
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
    echo "⚠️  Edit /opt/carb-calculator/.env with your ANTHROPIC_API_KEY before starting"
fi

# Start services
echo "🐳 Starting Docker Compose stack..."
docker compose up --build -d

echo ""
echo "✅ Deployment complete!"
echo ""
echo "Services running:"
echo "  - API:     http://localhost:3000"
echo "  - Grafana: http://localhost:3001 (admin / admin)"
echo "  - Loki:    http://localhost:3100"
echo ""
echo "Next steps:"
echo "  1. Edit /opt/carb-calculator/.env with your API keys"
echo "  2. Restart: docker compose -C /opt/carb-calculator restart app"
echo "  3. View logs: docker compose -C /opt/carb-calculator logs -f app"
echo ""
