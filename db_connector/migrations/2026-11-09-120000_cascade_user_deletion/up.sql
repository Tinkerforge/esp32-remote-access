-- Delete account-owned records whenever a user is deleted, including from
-- cleanup jobs and direct SQL. Tokens and device groupings already cascade.
ALTER TABLE "allowed_users"
    DROP CONSTRAINT "allowed_users_user_id_fkey",
    ADD CONSTRAINT "allowed_users_user_id_fkey"
        FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON DELETE CASCADE;

ALTER TABLE "wg_keys"
    DROP CONSTRAINT "wg_keys_user_id_fkey",
    ADD CONSTRAINT "wg_keys_user_id_fkey"
        FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON DELETE CASCADE;

ALTER TABLE "refresh_tokens"
    DROP CONSTRAINT "refresh_tokens_user_id_fkey",
    ADD CONSTRAINT "refresh_tokens_user_id_fkey"
        FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON DELETE CASCADE;

ALTER TABLE "recovery_tokens"
    DROP CONSTRAINT "recovery_tokens_user_id_fkey",
    ADD CONSTRAINT "recovery_tokens_user_id_fkey"
        FOREIGN KEY ("user_id") REFERENCES "users" ("id") ON DELETE CASCADE;

ALTER TABLE "verification"
    DROP CONSTRAINT "fk_users",
    ADD CONSTRAINT "fk_users"
        FOREIGN KEY ("user") REFERENCES "users" ("id") ON DELETE CASCADE;
