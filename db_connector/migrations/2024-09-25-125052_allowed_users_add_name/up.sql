-- Your SQL goes here

ALTER TABLE "allowed_users" ADD "key" BYTEA;
ALTER TABLE "allowed_users" ADD "name" BYTEA;
ALTER TABLE "allowed_users" ADD "note" BYTEA;
ALTER TABLE "allowed_users" DROP COLUMN "is_owner";
