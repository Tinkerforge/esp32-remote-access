-- This file should undo anything in `up.sql`

ALTER TABLE "allowed_users" drop COLUMN "key";
ALTER TABLE "allowed_users" drop COLUMN "name";
ALTER TABLE "allowed_users" drop COLUMN "note";
