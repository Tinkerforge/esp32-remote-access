CREATE INDEX refresh_tokens_user_id_expiration_id_idx
    ON refresh_tokens (user_id, expiration DESC, id DESC);
