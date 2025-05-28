-- This file should undo anything in `up.sql`
ALTER TABLE "authorization_tokens" DROP COLUMN "name";
ALTER TABLE "authorization_tokens" DROP COLUMN "created_at";
ALTER TABLE "authorization_tokens" DROP COLUMN "last_used_at";
