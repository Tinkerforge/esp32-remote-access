-- Your SQL goes here
CREATE TABLE "users" (
  "id" uuid PRIMARY KEY,
  "name" VARCHAR NOT NULL,
  "email" VARCHAR NOT NULL,
  "password" VARCHAR NOT NULL,
  "email_verified" BOOLEAN NOT NULL DEFAULT FALSE
);
