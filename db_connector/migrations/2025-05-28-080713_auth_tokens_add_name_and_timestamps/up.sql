-- Your SQL goes here
ALTER TABLE "authorization_tokens" ADD COLUMN "name" VARCHAR;
UPDATE "authorization_tokens" SET "name" = '';
ALTER TABLE "authorization_tokens" ALTER COLUMN "name" SET NOT NULL;

ALTER TABLE "authorization_tokens" ADD COLUMN "created_at" TIMESTAMP;
UPDATE "authorization_tokens" SET "created_at" = (now());
ALTER TABLE "authorization_tokens" ALTER COLUMN "created_at" SET NOT NULL;
ALTER TABLE "authorization_tokens" ADD COLUMN "last_used_at" TIMESTAMP;
