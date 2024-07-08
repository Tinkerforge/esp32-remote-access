use anyhow::Error;


pub fn generate_hash(password: &[u8], salt: &[u8], len: Option<usize>) -> anyhow::Result<Vec<u8>> {
    let output_len = len.unwrap_or(24);
    let params = match argon2::Params::new(19 * 1024, 2, 1, Some(output_len)) {
        Ok(p) => p,
        Err(_err) => {
            return Err(Error::msg("Failed to create params"));
        }
    };
    let argon2 = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut buf = vec![0u8; output_len];
    if let Err(_err) = argon2.hash_password_into(password, salt, &mut buf) {
        return Err(Error::msg("Failed to hash password"));
    }

    Ok(buf)
}
