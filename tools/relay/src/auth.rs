use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};

/// Token prefix for easy identification.
const TOKEN_PREFIX: &str = "tuic_";

/// Generate a new random bearer token with the `tuic_` prefix.
pub fn generate_token() -> String {
    let random_bytes = random_bytes::<32>();
    let encoded = base62_encode(&random_bytes);
    format!("{TOKEN_PREFIX}{encoded}")
}

/// Hash a token using argon2id for storage.
pub fn hash_token(token: &str) -> anyhow::Result<String> {
    let salt_bytes = random_bytes::<16>();
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|e| anyhow::anyhow!("salt encoding error: {e}"))?;
    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(token.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hash error: {e}"))?;
    Ok(hash.to_string())
}

/// Verify a token against its stored argon2id hash.
pub fn verify_token(token: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(token.as_bytes(), &parsed)
        .is_ok()
}

/// Simple base62 encoding (0-9, a-z, A-Z) for token generation.
fn base62_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 62] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    bytes
        .iter()
        .map(|b| ALPHABET[(*b as usize) % 62] as char)
        .collect()
}

/// Generate random bytes using getrandom (OS-level CSPRNG).
fn random_bytes<const N: usize>() -> [u8; N] {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).expect("OS random number generator failed");
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_generation_has_prefix() {
        let token = generate_token();
        assert!(token.starts_with("tuic_"));
        assert!(token.len() > 10);
    }

    #[test]
    fn token_hash_and_verify() {
        let token = generate_token();
        let hash = hash_token(&token).unwrap();
        assert!(verify_token(&token, &hash));
        assert!(!verify_token("wrong_token", &hash));
    }

    #[test]
    fn different_tokens_produce_different_hashes() {
        let t1 = generate_token();
        let t2 = generate_token();
        let h1 = hash_token(&t1).unwrap();
        let h2 = hash_token(&t2).unwrap();
        assert_ne!(h1, h2);
    }
}
