#!/bin/bash

# PostgreSQL Database Backup Script
# This script creates a database backup using docker exec and rotates old backups
# keeping only the last 365 backup files.

set -e

# Configuration
BACKUP_DIR="$HOME/backups"
CONTAINER_NAME="postgres"
DB_USER=$(grep -E '^POSTGRES_USER=' "$(dirname "$0")/../docker/.env" | cut -d'=' -f2-)
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
BACKUP_FILE="$BACKUP_DIR/postgres_backup_$TIMESTAMP.sql"
KEEP_DAYS=364

# Create backup directory if it doesn't exist
mkdir -p "$BACKUP_DIR"

# Perform the database backup
docker exec -t "$CONTAINER_NAME" pg_dump -U "$DB_USER" > "$BACKUP_FILE"


# Rotate backups - keep only the last 365 files
cd "$BACKUP_DIR"
BACKUP_COUNT=$(ls -1 postgres_backup_*.sql 2>/dev/null | wc -l)

    # Delete old backups, keeping only the most recent 365
ls -1t postgres_backup_*.sql | tail -n +$((KEEP_DAYS + 1)) | xargs rm -f
DELETED_COUNT=$((BACKUP_COUNT - KEEP_DAYS))
