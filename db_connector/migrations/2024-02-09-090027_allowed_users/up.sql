-- Your SQL goes here

CREATE TABLE "allowed_users" (
  "id" uuid PRIMARY KEY,
  "user_id" uuid NOT NULL REFERENCES users(id),
  "charger_id" INT NOT NULL REFERENCES chargers(id),
  "is_owner" bool NOT NULL
);
