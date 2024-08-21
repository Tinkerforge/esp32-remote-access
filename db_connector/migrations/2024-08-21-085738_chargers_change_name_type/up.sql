-- Your SQL goes here
ALTER TABLE "chargers" DROP COLUMN "name";
ALTER TABLE "chargers" ADD "name" BYTEA;
