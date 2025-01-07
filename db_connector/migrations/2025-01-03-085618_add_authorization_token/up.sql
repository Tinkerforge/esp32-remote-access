-- Your SQL goes here
CREATE TABLE "authorization_tokens"(
    "id" UUID PRIMARY KEY,
    "user_id" UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    "token" VARCHAR NOT NULL,
    "use_once" BOOL NOT NULL
);
