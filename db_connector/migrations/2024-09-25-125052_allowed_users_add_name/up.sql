-- Your SQL goes here

ALTER TABLE "allowed_users" ADD "name" VARCHAR;
ALTER TABLE "allowed_users" ADD "note" VARCHAR;
ALTER TABLE "allowed_users" DROP COLUMN "is_owner";
