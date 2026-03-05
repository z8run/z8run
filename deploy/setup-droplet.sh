#!/usr/bin/env bash
# ============================================================
# z8run – Setup script for Ubuntu 24.04 Droplet
# Run as root: bash setup-droplet.sh
# ============================================================
set -euo pipefail

echo "══════════════════════════════════════════════════"
echo "  z8run Droplet Setup"
echo "══════════════════════════════════════════════════"

# ── 1. System update ─────────────────────────────────────
echo "[1/6] Updating system..."
apt-get update && apt-get upgrade -y

# ── 2. Install Docker ────────────────────────────────────
echo "[2/6] Installing Docker..."
if ! command -v docker &>/dev/null; then
    curl -fsSL https://get.docker.com | sh
    systemctl enable docker
    systemctl start docker
    echo "  ✓ Docker installed"
else
    echo "  ✓ Docker already installed"
fi

# ── 3. Install Docker Compose plugin ─────────────────────
echo "[3/6] Verifying Docker Compose..."
if docker compose version &>/dev/null; then
    echo "  ✓ Docker Compose available"
else
    apt-get install -y docker-compose-plugin
    echo "  ✓ Docker Compose installed"
fi

# ── 4. Install PostgreSQL 16 ────────────────────────────
echo "[4/6] Installing PostgreSQL 16..."
if ! command -v psql &>/dev/null; then
    apt-get install -y gnupg2 lsb-release
    echo "deb http://apt.postgresql.org/pub/repos/apt $(lsb_release -cs)-pgdg main" \
        > /etc/apt/sources.list.d/pgdg.list
    curl -fsSL https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor \
        -o /etc/apt/trusted.gpg.d/postgresql.gpg
    apt-get update
    apt-get install -y postgresql-16
    systemctl enable postgresql
    systemctl start postgresql
    echo "  ✓ PostgreSQL 16 installed"
else
    echo "  ✓ PostgreSQL already installed"
fi

# ── 5. Configure PostgreSQL for z8run ────────────────────
echo "[5/6] Configuring PostgreSQL..."

# Prompt for password
read -sp "  Enter password for z8run DB user: " DB_PASS
echo

# Create user and database
sudo -u postgres psql -tc "SELECT 1 FROM pg_roles WHERE rolname='z8run'" | grep -q 1 || \
    sudo -u postgres psql -c "CREATE USER z8run WITH PASSWORD '${DB_PASS}';"
sudo -u postgres psql -tc "SELECT 1 FROM pg_catalog.pg_database WHERE datname='z8run'" | grep -q 1 || \
    sudo -u postgres psql -c "CREATE DATABASE z8run OWNER z8run;"

# Allow Docker containers to connect via host.docker.internal
PG_HBA=$(sudo -u postgres psql -tc "SHOW hba_file;" | xargs)
PG_CONF=$(sudo -u postgres psql -tc "SHOW config_file;" | xargs)

# Listen on Docker bridge + localhost
if ! grep -q "172.17.0.0/16" "$PG_HBA" 2>/dev/null; then
    echo "# z8run Docker access" >> "$PG_HBA"
    echo "host    z8run    z8run    172.17.0.0/16    scram-sha-256" >> "$PG_HBA"
    echo "host    z8run    z8run    172.18.0.0/16    scram-sha-256" >> "$PG_HBA"
fi

# Make Postgres listen on all interfaces (needed for Docker)
sed -i "s/^#listen_addresses.*/listen_addresses = 'localhost,172.17.0.1'/" "$PG_CONF"

systemctl restart postgresql
echo "  ✓ PostgreSQL configured (user: z8run, db: z8run)"

# ── 6. Firewall ──────────────────────────────────────────
echo "[6/6] Configuring firewall..."
if command -v ufw &>/dev/null; then
    ufw allow OpenSSH
    ufw allow 80/tcp
    ufw allow 443/tcp
    ufw --force enable
    echo "  ✓ Firewall: SSH + HTTP + HTTPS allowed"
fi

# ── Done ─────────────────────────────────────────────────
echo ""
echo "══════════════════════════════════════════════════"
echo "  ✓ Setup complete!"
echo ""
echo "  Next steps:"
echo "    1. Clone your repo:"
echo "       git clone https://github.com/tu-usuario/z8run.git"
echo "       cd z8run"
echo ""
echo "    2. Create .env file:"
echo "       cp .env.example .env"
echo "       # Edit .env — set POSTGRES_PASSWORD=${DB_PASS}"
echo "       #              set Z8_VAULT_SECRET=<random-string>"
echo "       #              set Z8_PUBLIC_PORT=80"
echo ""
echo "    3. Deploy:"
echo "       docker compose up -d --build"
echo ""
echo "    4. Point your Cloudflare DNS A record to:"
echo "       $(curl -s ifconfig.me)"
echo "══════════════════════════════════════════════════"
