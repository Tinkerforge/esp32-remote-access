-- Your SQL goes here
ALTER TABLE "verification" ADD "expiration" TIMESTAMP;
UPDATE "verification" SET "expiration" = (now());
ALTER TABLE "verification" ALTER COLUMN "expiration" SET NOT NULL;
