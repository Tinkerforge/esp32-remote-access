// @generated automatically by Diesel CLI.

diesel::table! {
    allowed_users (id) {
        id -> Uuid,
        user_id -> Uuid,
        charger_id -> Int4,
        is_owner -> Bool,
    }
}

diesel::table! {
    chargers (id) {
        id -> Int4,
        last_ip -> Nullable<Inet>,
        name -> Varchar,
        management_private -> Varchar,
        charger_pub -> Varchar,
        wg_charger_ip -> Inet,
        wg_server_ip -> Inet,
    }
}

diesel::table! {
    sessions (id) {
        id -> Uuid,
        user_id -> Uuid,
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
        secret_iv -> Bytea,
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
        charger_id -> Int4,
        in_use -> Bool,
        charger_pub -> Varchar,
        web_private -> Bytea,
        web_private_iv -> Bytea,
        web_address -> Inet,
        charger_address -> Inet,
        connection_no -> Int4,
    }
}

diesel::joinable!(allowed_users -> chargers (charger_id));
diesel::joinable!(allowed_users -> users (user_id));
diesel::joinable!(sessions -> users (user_id));
diesel::joinable!(verification -> users (user));
diesel::joinable!(wg_keys -> chargers (charger_id));
diesel::joinable!(wg_keys -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    allowed_users,
    chargers,
    sessions,
    users,
    verification,
    wg_keys,
);
