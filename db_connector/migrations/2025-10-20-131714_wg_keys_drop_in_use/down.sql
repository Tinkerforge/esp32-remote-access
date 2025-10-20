-- This file should undo anything in `up.sql`

ALTER TABLE "wg_keys" ADD COLUMN "in_use" BOOLEAN NOT NULL DEFAULT false;
