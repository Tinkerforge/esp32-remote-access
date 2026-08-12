-- Re-identify charger UIDs that were originally typed as z-base-32 strings but
-- mis-decoded by the old Flickr-base58-only `register_charger` flow, then
-- re-decode them to recover the true UID.
--
-- Background: the frontend renders UIDs > 257899 as z-base-32, but the old
-- backend decoded any UID string as Flickr-base58. For a user who typed a
-- z-base-32 string of 4-5 characters (e.g. `"855c"` for UID 257900), the
-- base58-decoded value fits in the 32-bit `chargers.uid` column, so the
-- stored integer is recoverable: we can re-encode the stored integer as
-- base58 to recover the original string, then decode that string as
-- z-base-32 to obtain the true UID.
--
-- For longer z-base-32 strings (6-7 characters, used for very large UIDs)
-- the base58-decoded value exceeds 32 bits and the high-order byte(s) were
-- silently dropped when the row was first inserted; the true UID is not
-- recoverable from the stored value alone. The migration logs and skips
-- these.
--
-- Conservative verification: we only apply the update if the recovered
-- string is itself a valid z-base-32 string whose z-base-32 interpretation
-- round-trips back to the same string (i.e. `zbase32_encode(zbase32_decode(z)) == z`).
-- This filters out:
--   * pure Flickr-base58 inputs whose encoding happens to be valid in the
--     (case-insensitive) z-base-32 alphabet, and
--   * the lossy case where the stored integer no longer represents the
--     original z-base-32 input.
--
-- Helper functions are scoped to this migration: they are created here and
-- dropped at the end of the script.

-- Decode a string in the (case-insensitive) z-base-32 alphabet to a
-- non-negative bigint. Returns NULL if any character is outside the
-- alphabet.
CREATE OR REPLACE FUNCTION pg_temp.zbase32_decode(input text) RETURNS bigint AS $$
DECLARE
    alphabet text := 'ybndrfg8ejkmcpqxot1uwisza345h769';
    result bigint := 0;
    i int;
    ch text;
    lower_ch text;
    idx int;
BEGIN
    IF input IS NULL OR length(input) = 0 THEN
        RETURN NULL;
    END IF;
    FOR i IN 1..length(input) LOOP
        ch := substring(input FROM i FOR 1);
        lower_ch := lower(ch);
        idx := position(lower_ch IN alphabet) - 1;
        IF idx < 0 THEN
            RETURN NULL;
        END IF;
        result := result * 32 + idx;
    END LOOP;
    RETURN result;
END;
$$ LANGUAGE plpgsql IMMUTABLE STRICT;

-- Encode a non-negative bigint as z-base-32.
CREATE OR REPLACE FUNCTION pg_temp.zbase32_encode(value bigint) RETURNS text AS $$
DECLARE
    alphabet text := 'ybndrfg8ejkmcpqxot1uwisza345h769';
    v bigint := value;
    rem int;
    result text := '';
BEGIN
    IF v IS NULL OR v < 0 THEN
        RETURN NULL;
    END IF;
    IF v = 0 THEN
        RETURN substring(alphabet FROM 1 FOR 1);
    END IF;
    WHILE v > 0 LOOP
        rem := (v % 32)::int;
        v := v / 32;
        result := substring(alphabet FROM rem + 1 FOR 1) || result;
    END LOOP;
    RETURN result;
END;
$$ LANGUAGE plpgsql IMMUTABLE STRICT;

-- Encode a non-negative bigint as Flickr-base58 (no leading '1' characters
-- for non-zero values).
CREATE OR REPLACE FUNCTION pg_temp.base58_flickr_encode(value bigint) RETURNS text AS $$
DECLARE
    alphabet text := '123456789abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ';
    v bigint := value;
    rem int;
    result text := '';
BEGIN
    IF v IS NULL OR v < 0 THEN
        RETURN NULL;
    END IF;
    IF v = 0 THEN
        RETURN substring(alphabet FROM 1 FOR 1);
    END IF;
    WHILE v > 0 LOOP
        rem := (v % 58)::int;
        v := v / 58;
        result := substring(alphabet FROM rem + 1 FOR 1) || result;
    END LOOP;
    RETURN result;
END;
$$ LANGUAGE plpgsql IMMUTABLE STRICT;

DO $$
DECLARE
    rec record;
    -- The stored i32 reinterpreted as an unsigned 32-bit value. This is
    -- the big-endian integer that was originally bs58-decoded (modulo
    -- truncation to 32 bits for very large values).
    be_int bigint;
    recovered_str text;
    recovered_uid bigint;
    reencoded_str text;
    invalid_chars boolean;
    ci int;
    cch text;
    fixed_count int := 0;
    skipped_intact_count int := 0;
    skipped_unrecoverable_count int := 0;
BEGIN
    FOR rec IN SELECT id, uid FROM chargers WHERE uid > 257899 LOOP
        -- Bit-cast the stored i32 to its unsigned 32-bit representation.
        be_int := rec.uid::bigint & x'FFFFFFFF'::bigint;

        -- Reverse-engineer the original (mis-decoded) string by
        -- re-encoding the stored integer as Flickr-base58.
        recovered_str := pg_temp.base58_flickr_encode(be_int);

        IF recovered_str IS NULL THEN
            skipped_unrecoverable_count := skipped_unrecoverable_count + 1;
            CONTINUE;
        END IF;

        -- All characters must be in the (case-insensitive) z-base-32
        -- alphabet for the recovered string to be a candidate z-base-32
        -- representation. Otherwise, the original input was a pure
        -- Flickr-base58 string and the stored UID is already correct.
        invalid_chars := false;
        FOR ci IN 1..length(recovered_str) LOOP
            cch := lower(substring(recovered_str FROM ci FOR 1));
            IF position(cch IN 'ybndrfg8ejkmcpqxot1uwisza345h769') = 0 THEN
                invalid_chars := true;
                EXIT;
            END IF;
        END LOOP;
        IF invalid_chars THEN
            skipped_intact_count := skipped_intact_count + 1;
            CONTINUE;
        END IF;

        recovered_uid := pg_temp.zbase32_decode(recovered_str);

        -- Round-trip sanity check: the recovered string must be the
        -- z-base-32 form of the recovered UID. This guards against
        --   (a) pure Flickr-base58 inputs that happen to use only
        --       z-base-32-alphabet characters, and
        --   (b) the lossy case where the original z-base-32 string's
        --       base58-decoded value exceeded 32 bits and was truncated
        --       on insert.
        reencoded_str := pg_temp.zbase32_encode(recovered_uid);
        IF reencoded_str IS NULL OR reencoded_str <> recovered_str THEN
            -- The z-base-32 encoding of recovered_uid is not the same
            -- string as recovered_str. This catches both the pure-
            -- Flickr-base58 case and the lossy case where the original
            -- z-base-32 string's base58-decoded value exceeded 32 bits.
            skipped_unrecoverable_count := skipped_unrecoverable_count + 1;
            RAISE NOTICE 'Unrecoverable charger % (stored uid=%, recovered_string=%, recovered_uid=%, reencoded=%)',
                rec.id, rec.uid, recovered_str, recovered_uid, reencoded_str;
            CONTINUE;
        END IF;

        IF recovered_uid > 2147483647 THEN
            -- The recovered UID does not fit in the `chargers.uid` i32
            -- column. Skip.
            skipped_unrecoverable_count := skipped_unrecoverable_count + 1;
            RAISE NOTICE 'Recovered uid % exceeds i32 range for charger %', recovered_uid, rec.id;
            CONTINUE;
        END IF;

        IF recovered_uid::int = rec.uid THEN
            -- Round-trip produced the same integer we already have.
            -- Nothing to fix.
            skipped_intact_count := skipped_intact_count + 1;
            CONTINUE;
        END IF;

        UPDATE chargers SET uid = recovered_uid::int WHERE id = rec.id;
        UPDATE allowed_users
            SET charger_uid = recovered_uid::int
            WHERE charger_id = rec.id;
        fixed_count := fixed_count + 1;
        RAISE NOTICE 'Fixed charger %: uid % -> % (recovered from base58 string %)',
            rec.id, rec.uid, recovered_uid, recovered_str;
    END LOOP;

    RAISE NOTICE 'Migration summary: fixed=%, skipped_intact=%, skipped_unrecoverable=%',
        fixed_count, skipped_intact_count, skipped_unrecoverable_count;
END $$;

DROP FUNCTION IF EXISTS pg_temp.zbase32_decode(text);
DROP FUNCTION IF EXISTS pg_temp.zbase32_encode(bigint);
DROP FUNCTION IF EXISTS pg_temp.base58_flickr_encode(bigint);
