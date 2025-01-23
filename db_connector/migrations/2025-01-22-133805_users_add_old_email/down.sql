-- This file should undo anything in `up.sql`
ALTER TABLE "users" DROP COLUMN "old_email";
ALTER TABLE "users" DROP COLUMN "old_delivery_email";
