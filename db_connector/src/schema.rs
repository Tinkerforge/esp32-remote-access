// @generated automatically by Diesel CLI.

diesel::table! {
    allowed_users (id) {
        id -> Uuid,
        user_id -> Uuid,
        valid -> Bool,
        name -> Nullable<Bytea>,
        note -> Nullable<Bytea>,
        charger_uid -> Int4,
        charger_id -> Uuid,
    }
}

diesel::table! {
    chargers (id) {
        password -> Varchar,
        management_private -> Varchar,
        charger_pub -> Varchar,
        psk -> Varchar,
        wg_charger_ip -> Inet,
        wg_server_ip -> Inet,
        webinterface_port -> Int4,
        firmware_version -> Varchar,
        name -> Nullable<Bytea>,
        uid -> Int4,
        id -> Uuid,
    }
}

diesel::table! {
    recovery_tokens (id) {
        id -> Uuid,
        user_id -> Uuid,
        created -> Int8,
    }
}

diesel::table! {
    refresh_tokens (id) {
        id -> Uuid,
        user_id -> Uuid,
        expiration -> Int8,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        name -> Varchar,
        email -> Varchar,
        #[sql_name = "login-key"]
        login_key -> Varchar,
        email_verified -> Bool,
        secret -> Bytea,
        secret_nonce -> Bytea,
        #[sql_name = "secret-salt"]
        secret_salt -> Bytea,
        #[sql_name = "login-salt"]
        login_salt -> Bytea,
    }
}

diesel::table! {
    verification (id) {
        id -> Uuid,
        user -> Uuid,
    }
}

diesel::table! {
    wg_keys (id) {
        id -> Uuid,
        user_id -> Uuid,
        in_use -> Bool,
        charger_pub -> Varchar,
        web_private -> Bytea,
        psk -> Bytea,
        web_address -> Inet,
        charger_address -> Inet,
        connection_no -> Int4,
        charger_id -> Uuid,
    }
}

diesel::joinable!(allowed_users -> chargers (charger_id));
diesel::joinable!(allowed_users -> users (user_id));
diesel::joinable!(recovery_tokens -> users (user_id));
diesel::joinable!(refresh_tokens -> users (user_id));
diesel::joinable!(verification -> users (user));
diesel::joinable!(wg_keys -> chargers (charger_id));
diesel::joinable!(wg_keys -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    allowed_users,
    chargers,
    recovery_tokens,
    refresh_tokens,
    users,
    verification,
    wg_keys,
);
