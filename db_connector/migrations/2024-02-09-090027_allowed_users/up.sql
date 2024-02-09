-- Your SQL goes here

CREATE TABLE "allowed_users" (
  "id" uuid PRIMARY KEY,
  "user" uuid NOT NULL REFERENCES users(id),
  "charger" varchar NOT NULL REFERENCES chargers(id),
  "is_owner" bool NOT NULL
);
