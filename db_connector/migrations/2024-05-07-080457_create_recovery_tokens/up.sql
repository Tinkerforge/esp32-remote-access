-- Your SQL goes here
CREATE TABLE recovery_tokens (
    "id" uuid PRIMARY KEY,
    "user_id" uuid NOT NULL REFERENCES users(id),
    "created" BIGINT NOT NULL
)
