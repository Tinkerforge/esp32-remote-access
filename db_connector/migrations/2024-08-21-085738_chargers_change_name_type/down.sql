-- This file should undo anything in `up.sql`
ALTER TABLE "chargers" DROP COLUMN "name";
ALTER TABLE "chargers" ADD "name" VARCHAR;
