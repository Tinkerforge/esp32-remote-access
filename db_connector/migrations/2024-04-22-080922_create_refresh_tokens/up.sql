-- Your SQL goes here

CREATE TABLE "refresh_tokens" (
    "id" uuid PRIMARY KEY,
    "user_id" uuid NOT NULL REFERENCES users(id),
    "expiration" BIGINT NOT NULL
);
