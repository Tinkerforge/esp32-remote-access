-- This file should undo anything in `up.sql`

ALTER TABLE "allowed_users" DROP CONSTRAINT "allowed_users_charger_id_fkey";
-- ALTER TABLE "allowed_users" DROP CONSTRAINT "allowed_users_charger_id_fkey";
ALTER TABLE "allowed_users" DROP COLUMN "charger_id";
ALTER TABLE "allowed_users" ADD "charger_id" INT;
UPDATE "allowed_users" T SET "charger_id" = (T.charger_uid);
ALTER TABLE "allowed_users" ALTER COLUMN "charger_id" SET NOT NULL;
ALTER TABLE "allowed_users" DROP "charger_uid";

ALTER TABLE "wg_keys" DROP CONSTRAINT "wg_keys_charger_id_fkey";
ALTER TABLE "wg_keys" ADD "charger_uid" INT;
UPDATE "wg_keys" T SET "charger_uid" = (SELECT "uid" FROM "chargers" WHERE "id" = T.charger_id);
ALTER TABLE "wg_keys" DROP COLUMN "charger_id";
ALTER TABLE "wg_keys" ADD "charger_id" INT;
UPDATE "wg_keys" T SET "charger_id" = (T.charger_uid);
ALTER TABLE "wg_keys" ALTER COLUMN "charger_id" SET NOT NULL;
ALTER TABLE "wg_keys" DROP COLUMN "charger_uid";

ALTER TABLE "chargers" DROP COLUMN "id";
ALTER TABLE "chargers" ADD "id" INT;
UPDATE "chargers" T SET "id" = (T.uid);
ALTER TABLE "chargers" ADD PRIMARY KEY ("id");
ALTER TABLE "chargers" DROP COLUMN "uid";

ALTER TABLE "allowed_users" ADD CONSTRAINT "allowed_users_charger_id_fkey" FOREIGN KEY ("charger_id") REFERENCES "chargers"("id");
ALTER TABLE "wg_keys" ADD CONSTRAINT "wg_keys_charger_id_fkey" FOREIGN KEY ("charger_id") REFERENCES "chargers"("id");
