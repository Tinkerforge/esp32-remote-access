-- Your SQL goes here
CREATE TABLE "device_groupings"(
    "id" UUID PRIMARY KEY,
    "name" VARCHAR NOT NULL,
    "user_id" UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE
);
