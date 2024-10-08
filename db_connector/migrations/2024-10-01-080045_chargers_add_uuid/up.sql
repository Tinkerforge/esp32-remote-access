-- Your SQL goes here

ALTER TABLE "allowed_users" DROP CONSTRAINT "allowed_users_charger_id_fkey";
ALTER TABLE "allowed_users" ADD "charger_uid" INT;

ALTER TABLE "wg_keys" DROP CONSTRAINT "wg_keys_charger_id_fkey";
ALTER TABLE "wg_keys" ADD "charger_uid" INT;

UPDATE "allowed_users" T SET "charger_uid" = (T.charger_id);
ALTER TABLE "allowed_users" ALTER COLUMN "charger_uid" SET NOT NULL;
UPDATE "wg_keys" T SET "charger_uid" = (T.charger_id);

ALTER TABLE "chargers" ADD "uuid" UUID;
UPDATE "chargers" SET uuid = (gen_random_uuid());

ALTER TABLE "allowed_users" DROP COLUMN "charger_id";
ALTER TABLE "allowed_users" ADD "charger_id" UUID;
UPDATE "allowed_users" T SET "charger_id" = (SELECT "uuid" FROM "chargers" WHERE id = T.charger_uid);
ALTER TABLE "allowed_users" ALTER COLUMN "charger_id" SET NOT NULL;

ALTER TABLE "wg_keys" DROP COLUMN "charger_id";
ALTER TABLE "wg_keys" ADD "charger_id" UUID;
UPDATE "wg_keys" T SET "charger_id" = (SELECT "uuid" FROM "chargers" WHERE id = T.charger_uid);
ALTER TABLE "wg_keys" ALTER COLUMN "charger_id" SET NOT NULL;
ALTER TABLE "wg_keys" DROP COLUMN "charger_uid";


ALTER TABLE "chargers" ADD "uid" INT;
UPDATE "chargers" T SET "uid" = (T.id);
ALTER TABLE "chargers" ALTER COLUMN "uid" SET NOT NULL;
ALTER TABLE "chargers" DROP "id";
ALTER TABLE "chargers" ADD "id" UUID;
UPDATE "chargers" T SET "id" = (T.uuid);
ALTER TABLE "chargers" ADD PRIMARY KEY ("id");
ALTER TABLE "chargers" DROP COLUMN "uuid";

ALTER TABLE "allowed_users" ADD CONSTRAINT "allowed_users_charger_id_fkey" FOREIGN KEY ("charger_id") REFERENCES "chargers" ("id");
ALTER TABLE "wg_keys" ADD CONSTRAINT "wg_keys_charger_id_fkey" FOREIGN KEY ("charger_id") REFERENCES "chargers" ("id");
