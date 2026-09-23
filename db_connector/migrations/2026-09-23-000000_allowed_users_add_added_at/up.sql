-- Existing associations predate this column, so their original add time is unknown.
ALTER TABLE "allowed_users" ADD COLUMN "added_at" TIMESTAMP;
