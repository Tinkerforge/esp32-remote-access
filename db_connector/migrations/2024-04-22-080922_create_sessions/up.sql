-- Your SQL goes here

CREATE TABLE "sessions" (
    "id" uuid PRIMARY KEY,
    "user_id" uuid NOT NULL REFERENCES users(id)
);
