use diesel::{pg::Pg, PgConnection};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub mod models;
pub mod schema;

#[cfg(test)]
use diesel::migration::MigrationSource;

pub type Pool = diesel::r2d2::Pool<diesel::r2d2::ConnectionManager<PgConnection>>;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub fn run_migrations(
    connection: &mut impl MigrationHarness<Pg>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    connection.run_pending_migrations(MIGRATIONS)?;

    Ok(())
}

/**
 * Create db connection pool
 */
pub fn get_connection_pool() -> Pool {
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = diesel::r2d2::ConnectionManager::<PgConnection>::new(url);
    Pool::builder()
        .max_size(90)
        .test_on_check_out(true)
        .build(manager)
        .expect("Could not build connection pool")
}

pub fn test_connection_pool() -> Pool {
    dotenvy::dotenv().ok();
    let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let manager = diesel::r2d2::ConnectionManager::<PgConnection>::new(url);
    Pool::builder()
        .test_on_check_out(true)
        .max_size(1)
        .build(manager)
        .expect("Could not build connection pool")
}

#[cfg(test)]
mod tests {
    //! Tests for individual migrations. Each test exercises a migration's
    //! up.sql against a freshly-seeded chunk of the schema, then asserts
    //! the post-migration state. The tests use the embedded migrations
    //! list for the schema setup and execute the migration's SQL directly
    //! so the test does not depend on the migration having been recorded
    //! as "applied" in `__diesel_schema_migrations`.

    use super::*;
    use diesel::prelude::*;

    /// Build a charger row using only the fields the data migration
    /// actually touches (the migration only reads/updates `uid`, so
    /// the other columns can take any values that satisfy NOT NULL).
    fn insert_test_charger(
        conn: &mut PgConnection,
        charger_id: uuid::Uuid,
        uid: i32,
        charger_pub: &str,
        wg_charger_ip: &str,
        wg_server_ip: &str,
        psk: &str,
    ) {
        use crate::schema::chargers::dsl as c;
        let wg_charger_ip: ipnetwork::IpNetwork = wg_charger_ip.parse().unwrap();
        let wg_server_ip: ipnetwork::IpNetwork = wg_server_ip.parse().unwrap();
        diesel::insert_into(c::chargers)
            .values((
                c::id.eq(charger_id),
                c::uid.eq(uid),
                c::password.eq("test-password"),
                c::name.eq(None::<Vec<u8>>),
                c::management_private.eq("test-mgmt-priv"),
                c::charger_pub.eq(charger_pub),
                c::wg_charger_ip.eq(wg_charger_ip),
                c::wg_server_ip.eq(wg_server_ip),
                c::psk.eq(psk.to_owned()),
                c::webinterface_port.eq(0),
                c::firmware_version.eq("test"),
                c::last_state_change.eq(None::<chrono::NaiveDateTime>),
                c::device_type.eq(None::<String>),
                c::mtu.eq(None::<i32>),
                c::last_charge_log_upload_hash.eq(Vec::<Option<Vec<u8>>>::new()),
            ))
            .execute(conn)
            .expect("insert test charger");
    }

    fn charger_uid(conn: &mut PgConnection, charger_id: uuid::Uuid) -> Option<i32> {
        use crate::schema::chargers::dsl as c;
        c::chargers
            .filter(c::id.eq(charger_id))
            .select(c::uid)
            .first::<i32>(conn)
            .optional()
            .expect("query charger uid")
    }

    fn insert_test_allowed_user(
        conn: &mut PgConnection,
        allowed_id: uuid::Uuid,
        user_id: uuid::Uuid,
        charger_id: uuid::Uuid,
        charger_uid_value: i32,
    ) {
        use crate::schema::allowed_users::dsl as a;
        diesel::insert_into(a::allowed_users)
            .values((
                a::id.eq(allowed_id),
                a::user_id.eq(user_id),
                a::valid.eq(true),
                a::name.eq(None::<String>),
                a::note.eq(None::<String>),
                a::charger_uid.eq(charger_uid_value),
                a::charger_id.eq(charger_id),
            ))
            .execute(conn)
            .expect("insert test allowed_user");
    }

    fn allowed_user_uid(
        conn: &mut PgConnection,
        allowed_id: uuid::Uuid,
    ) -> Option<i32> {
        use crate::schema::allowed_users::dsl as a;
        a::allowed_users
            .filter(a::id.eq(allowed_id))
            .select(a::charger_uid)
            .first::<i32>(conn)
            .optional()
            .expect("query allowed_user uid")
    }

    fn insert_test_user(conn: &mut PgConnection, user_id: uuid::Uuid, email: &str) {
        use crate::schema::users::dsl as u;
        // All the bytea columns must be non-empty for NOT NULL to hold;
        // we use a single zero byte as a placeholder.
        let empty = Vec::<u8>::new();
        diesel::insert_into(u::users)
            .values((
                u::id.eq(user_id),
                u::name.eq(format!("test-user-{user_id}")),
                u::email.eq(email.to_owned()),
                u::login_key.eq("test-login-key".to_owned()),
                u::email_verified.eq(true),
                u::secret.eq(&empty),
                u::secret_nonce.eq(&empty),
                u::secret_salt.eq(&empty),
                u::login_salt.eq(&empty),
                u::delivery_email.eq(None::<String>),
                u::old_email.eq(None::<String>),
                u::old_delivery_email.eq(None::<String>),
            ))
            .execute(conn)
            .expect("insert test user");
    }

    fn delete_test_charger(conn: &mut PgConnection, charger_id: uuid::Uuid) {
        use crate::schema::allowed_users::dsl as a;
        use crate::schema::chargers::dsl as c;
        // allowed_users has a FK to chargers, so delete allowed_users first.
        let _ = diesel::delete(a::allowed_users.filter(a::charger_id.eq(charger_id)))
            .execute(conn);
        diesel::delete(c::chargers.filter(c::id.eq(charger_id)))
            .execute(conn)
            .expect("delete test charger");
    }

    /// Run the migration's SQL directly. We don't go through the
    /// migration framework because the migration is already recorded as
    /// applied in `__diesel_schema_migrations` after the CI step
    /// `diesel migration run`; we only want to exercise the SQL.
    fn run_migration_sql(conn: &mut PgConnection, sql: &str) {
        use diesel::connection::SimpleConnection;
        conn.batch_execute(sql).expect("migration SQL");
    }

    /// Migrate the schema to the pre-state of the new data migration
    /// (revert everything and re-apply everything except the new
    /// migration). This is the only safe way to insert test data that
    /// matches what the migration expects, without changing the schema
    /// or polluting other tests.
    ///
    /// `MIGRATIONS` is iterated, `revert_all_migrations` reverts in
    /// reverse applied order, and `run_migrations` (which takes a
    /// slice) re-applies just the prefix we want.
    fn revert_to_pre_migration_state(conn: &mut PgConnection, new_migration_name_substr: &str) {
        let all = MIGRATIONS.migrations().expect("list migrations");
        let new_idx = all
            .iter()
            .position(|m| m.name().to_string().contains(new_migration_name_substr))
            .unwrap_or_else(|| panic!("could not find migration containing {new_migration_name_substr:?}"));

        // Drop test data first to avoid FK violations. Delete children
        // before their parents: device_grouping_members and allowed_users
        // and wg_keys depend on chargers, which depend on nothing.
        // Tokens and groupings depend on users.
        use crate::schema::device_grouping_members::dsl as dgm;
        let _ = diesel::delete(dgm::device_grouping_members).execute(conn);
        use crate::schema::allowed_users::dsl as a;
        let _ = diesel::delete(a::allowed_users).execute(conn);
        use crate::schema::wg_keys::dsl as w;
        let _ = diesel::delete(w::wg_keys).execute(conn);
        use crate::schema::refresh_tokens::dsl as rt;
        let _ = diesel::delete(rt::refresh_tokens).execute(conn);
        use crate::schema::recovery_tokens::dsl as rec;
        let _ = diesel::delete(rec::recovery_tokens).execute(conn);
        use crate::schema::verification::dsl as v;
        let _ = diesel::delete(v::verification).execute(conn);
        use crate::schema::authorization_tokens::dsl as at;
        let _ = diesel::delete(at::authorization_tokens).execute(conn);
        use crate::schema::device_groupings::dsl as dg;
        let _ = diesel::delete(dg::device_groupings).execute(conn);
        use crate::schema::chargers::dsl as c;
        let _ = diesel::delete(c::chargers).execute(conn);
        use crate::schema::users::dsl as u;
        let _ = diesel::delete(u::users).execute(conn);

        // Revert all migrations (in reverse applied order).
        conn.revert_all_migrations(MIGRATIONS)
            .expect("revert all migrations");

        // Re-apply everything before the new migration.
        conn.run_migrations(&all[..new_idx])
            .expect("re-apply pre migrations");
    }

    /// After the test, restore the schema to the post-migration state
    /// by re-applying everything that was reverted.
    fn restore_post_migration_state(conn: &mut PgConnection, new_migration_name_substr: &str) {
        let all = MIGRATIONS.migrations().expect("list migrations");
        let new_idx = all
            .iter()
            .position(|m| m.name().to_string().contains(new_migration_name_substr))
            .unwrap_or_else(|| panic!("could not find migration containing {new_migration_name_substr:?}"));

        // Everything from new_idx onward is currently un-applied; re-apply
        // them. This includes the new migration (whose down.sql is a
        // no-op) and any later migrations.
        conn.run_migrations(&all[new_idx..])
            .expect("re-apply post migrations");
    }

    const MIGRATION_SQL: &str = include_str!(
        "../migrations/2026-08-11-122701_reparse_misdecoded_zbase32_uids/up.sql"
    );

    /// A UID whose z-base-32 string has a bs58-decoded value that fits in
    /// 32 bits. We can round-trip the original (mis-decoded) string
    /// from the stored integer and recover the true UID.
    const RECOVERABLE_TRUE_UID: i32 = 257_900;

    /// The bs58 Flickr encoding of `"855c"` (z-base-32 of 257900),
    /// i.e. the value that the old base58-only decoder would have
    /// produced and stored in `chargers.uid` instead of 257900.
    const RECOVERABLE_MISDECODED_UID: i32 = 1_379_483;

    /// The value stored by the old decoder when a user typed "d999999"
    /// (z-base-32 of u32::MAX) for UID u32::MAX. The original z-base-32
    /// string's bs58-decoded value exceeds 32 bits, so the high-order
    /// byte was silently dropped when the row was first inserted; the
    /// migration cannot recover the true UID and must leave the stored
    /// value unchanged.
    const LOSSY_MISDECODED_UID: i32 = -1_689_183_048;

    /// A UID > 257899 that should also remain unchanged because the
    /// bs58 encoding of the stored integer contains characters z-base-32
    /// rejects (the original was a pure Flickr-base58 input).
    const BASE58_INTACT_TRUE_UID: i32 = 100_000;

    #[test]
    fn reparse_misdecoded_zbase32_uids_migration() {
        let pool = test_connection_pool();
        let mut conn = pool.get().expect("connection");

        // Set up the schema as it existed just before the new migration
        // ran, so the migration's WHERE clause (`uid > 257899`) applies
        // to the chargers we're about to insert.
        revert_to_pre_migration_state(&mut conn, "reparse_misdecoded_zbase32_uids");

        // Insert fixtures.
        // Each charger that has a corresponding `allowed_users` row
        // needs a backing user (allowed_users.user_id has a FK to
        // users.id), so we create those first.
        let user_a = uuid::Uuid::new_v4();
        insert_test_user(&mut conn, user_a, "test-a@example.com");
        let user_b = uuid::Uuid::new_v4();
        insert_test_user(&mut conn, user_b, "test-b@example.com");
        let user_c = uuid::Uuid::new_v4();
        insert_test_user(&mut conn, user_c, "test-c@example.com");

        let charger_a = uuid::Uuid::new_v4(); // mis-decoded, should be corrected
        insert_test_charger(
            &mut conn,
            charger_a,
            RECOVERABLE_MISDECODED_UID,
            "pub-a",
            "10.1.0.1/24",
            "10.1.0.2/24",
            "",
        );
        let allowed_a = uuid::Uuid::new_v4();
        insert_test_allowed_user(&mut conn, allowed_a, user_a, charger_a, RECOVERABLE_MISDECODED_UID);

        let charger_b = uuid::Uuid::new_v4(); // lossy z-base-32, should remain unchanged
        insert_test_charger(
            &mut conn,
            charger_b,
            LOSSY_MISDECODED_UID,
            "pub-b",
            "10.2.0.1/24",
            "10.2.0.2/24",
            "",
        );
        let allowed_b = uuid::Uuid::new_v4();
        insert_test_allowed_user(&mut conn, allowed_b, user_b, charger_b, LOSSY_MISDECODED_UID);

        let charger_c = uuid::Uuid::new_v4(); // pure base58, should remain unchanged
        insert_test_charger(
            &mut conn,
            charger_c,
            BASE58_INTACT_TRUE_UID,
            "pub-c",
            "10.3.0.1/24",
            "10.3.0.2/24",
            "",
        );
        let allowed_c = uuid::Uuid::new_v4();
        insert_test_allowed_user(&mut conn, allowed_c, user_c, charger_c, BASE58_INTACT_TRUE_UID);

        // A charger below the threshold should be ignored entirely.
        let charger_low = uuid::Uuid::new_v4();
        insert_test_charger(
            &mut conn,
            charger_low,
            123,
            "pub-low",
            "10.4.0.1/24",
            "10.4.0.2/24",
            "",
        );

        // A separate, unowned charger row that should also be left
        // alone (its allowed_users don't have anything to update).
        let charger_unowned = uuid::Uuid::new_v4();
        insert_test_charger(
            &mut conn,
            charger_unowned,
            BASE58_INTACT_TRUE_UID,
            "pub-unowned",
            "10.5.0.1/24",
            "10.5.0.2/24",
            "",
        );

        // Run the migration.
        run_migration_sql(&mut conn, MIGRATION_SQL);

        // Assertions.
        assert_eq!(
            charger_uid(&mut conn, charger_a),
            Some(RECOVERABLE_TRUE_UID),
            "charger A (mis-decoded z-base-32) should have been corrected",
        );
        assert_eq!(
            allowed_user_uid(&mut conn, allowed_a),
            Some(RECOVERABLE_TRUE_UID),
            "charger A's allowed_user row should have been corrected",
        );

        assert_eq!(
            charger_uid(&mut conn, charger_b),
            Some(LOSSY_MISDECODED_UID),
            "charger B (lossy z-base-32, info lost) should be left unchanged",
        );
        assert_eq!(
            allowed_user_uid(&mut conn, allowed_b),
            Some(LOSSY_MISDECODED_UID),
            "charger B's allowed_user row should be left unchanged",
        );

        assert_eq!(
            charger_uid(&mut conn, charger_c),
            Some(BASE58_INTACT_TRUE_UID),
            "charger C (pure Flickr-base58) should be left unchanged",
        );
        assert_eq!(
            allowed_user_uid(&mut conn, allowed_c),
            Some(BASE58_INTACT_TRUE_UID),
            "charger C's allowed_user row should be left unchanged",
        );

        assert_eq!(
            charger_uid(&mut conn, charger_low),
            Some(123),
            "charger with uid below threshold should be left unchanged",
        );

        assert_eq!(
            charger_uid(&mut conn, charger_unowned),
            Some(BASE58_INTACT_TRUE_UID),
            "unowned charger with uid below the lossy threshold should be left unchanged",
        );

        // Idempotency: re-running the migration on the corrected rows
        // must not flip them back. (The migration is naturally
        // idempotent because once the uid is the true value, the
        // recovered string no longer differs from the stored value.)
        run_migration_sql(&mut conn, MIGRATION_SQL);
        assert_eq!(charger_uid(&mut conn, charger_a), Some(RECOVERABLE_TRUE_UID));

        // Cleanup.
        delete_test_charger(&mut conn, charger_a);
        delete_test_charger(&mut conn, charger_b);
        delete_test_charger(&mut conn, charger_c);
        delete_test_charger(&mut conn, charger_low);
        delete_test_charger(&mut conn, charger_unowned);
        // The user rows were created with no other tables referencing them
        // (the migration does not touch users), so a direct delete works.
        {
            use crate::schema::users::dsl as u;
            diesel::delete(u::users)
                .filter(u::id.eq_any(vec![user_a, user_b, user_c]))
                .execute(&mut conn)
                .expect("delete test users");
        }

        // Restore the post-migration schema so subsequent tests see the
        // same state they would have seen without this test.
        restore_post_migration_state(&mut conn, "reparse_misdecoded_zbase32_uids");
    }
}
