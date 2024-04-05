-- Your SQL goes here
CREATE TABLE "users" (
  "id" uuid PRIMARY KEY,
  "name" VARCHAR NOT NULL,
  "email" VARCHAR NOT NULL,
  "login-key" VARCHAR NOT NULL,
  "email_verified" BOOLEAN NOT NULL DEFAULT FALSE,
  "secret" BYTEA NOT NULL,
  "secret-salt" BYTEA NOT NULL,
  "login-salt" BYTEA NOT NULL
);
