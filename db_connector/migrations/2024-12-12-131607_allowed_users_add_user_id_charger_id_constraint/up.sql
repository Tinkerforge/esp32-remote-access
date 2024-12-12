-- Your SQL goes here

ALTER TABLE "allowed_users" ADD CONSTRAINT "user_id_charger_id" UNIQUE ("user_id", "charger_id");
