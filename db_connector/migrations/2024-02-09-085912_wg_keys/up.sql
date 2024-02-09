-- Your SQL goes here

CREATE TABLE "wg_keys" (
  "id" uuid PRIMARY KEY,
  "charger" VARCHAR NOT NULL REFERENCES chargers(id),
  "in_use" BOOLEAN NOT NULL,
  "charger_pub" VARCHAR NOT NULL,
  "user_private" VARCHAR NOT NULL
);

