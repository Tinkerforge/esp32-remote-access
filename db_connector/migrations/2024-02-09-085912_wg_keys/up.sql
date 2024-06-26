-- Your SQL goes here

CREATE TABLE "wg_keys" (
  "id" uuid PRIMARY KEY,
  "user_id" uuid NOT NULL REFERENCES users(id),
  "charger_id" INT NOT NULL REFERENCES chargers(id),
  "in_use" BOOLEAN NOT NULL,
  "charger_pub" VARCHAR NOT NULL,
  "web_private" BYTEA NOT NULL,
  "psk" BYTEA NOT NULL,
  "web_address" INET NOT NULL,
  "charger_address" INET NOT NULL,
  "connection_no" INT NOT NULL
);
