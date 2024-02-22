// @generated automatically by Diesel CLI.

diesel::table! {
    allowed_users (id) {
        id -> Uuid,
        user -> Uuid,
        charger -> Varchar,
        is_owner -> Bool,
    }
}

diesel::table! {
    chargers (id) {
        id -> Varchar,
        last_ip -> Nullable<Varchar>,
        name -> Varchar,
    }
}

diesel::table! {
    users (id) {
        id -> Uuid,
        name -> Varchar,
        email -> Varchar,
        password -> Varchar,
        email_verified -> Bool,
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
        charger -> Varchar,
        in_use -> Bool,
        charger_pub -> Varchar,
        user_private -> Varchar,
    }
}

diesel::joinable!(allowed_users -> chargers (charger));
diesel::joinable!(allowed_users -> users (user));
diesel::joinable!(verification -> users (user));
diesel::joinable!(wg_keys -> chargers (charger));
diesel::joinable!(wg_keys -> users (user_id));

diesel::allow_tables_to_appear_in_same_query!(
    allowed_users,
    chargers,
    users,
    verification,
    wg_keys,
);
