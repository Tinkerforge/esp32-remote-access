-- This file should undo anything in `up.sql`

ALTER TABLE wg_keys DROP CONSTRAINT "wg_keys_user_id_fkey";
ALTER TABLE wg_keys ADD CONSTRAINT "wg_keys_user_id_fkey" FOREIGN KEY (user_id) REFERENCES users(id);
