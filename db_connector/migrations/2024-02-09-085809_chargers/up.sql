-- Your SQL goes here

CREATE TABLE "chargers" (
  "id" INT PRIMARY KEY,
  "password" VARCHAR NOT NULL,
  "last_ip" INET,
  "name" VARCHAR NOT NULL,
  "management_private" VARCHAR NOT NULL,
  "charger_pub" VARCHAR NOT NULL,
  "psk" VARCHAR NOT NULL,
  "wg_charger_ip" INET NOT NULL,
  "wg_server_ip" INET NOT NULL
);
