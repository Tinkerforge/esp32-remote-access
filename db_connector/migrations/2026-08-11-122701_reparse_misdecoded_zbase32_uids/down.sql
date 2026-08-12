-- This migration is not reversible: once we have re-decoded a UID as
-- z-base-32 and overwritten the stored value, the original (mis-decoded)
-- Flickr-base58 interpretation cannot be reconstructed without external
-- knowledge of which string the user originally typed. The data changed by
-- `up.sql` is therefore intentionally permanent.
SELECT 1;
