-- Your SQL goes here

CREATE TABLE "wg_keys" (
  "id" uuid PRIMARY KEY,
  "user_id" uuid NOT NULL REFERENCES users(id),
  "charger_id" VARCHAR NOT NULL REFERENCES chargers(id),
  "in_use" BOOLEAN NOT NULL,
  "charger_pub" VARCHAR NOT NULL,
  "user_private" VARCHAR NOT NULL,
  "web_address" INET NOT NULL,
  "charger_address" INET NOT NULL
);
