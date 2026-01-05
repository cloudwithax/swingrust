//! Authentication utilities

use anyhow::Result;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use pbkdf2::pbkdf2_hmac;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH};
use subtle::ConstantTimeEq;

use crate::config::UserConfig;

const PBKDF2_ITERATIONS: u32 = 100_000;
const HASH_LENGTH: usize = 32;

/// user identity stored in jwt sub claim matching upstream python structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIdentity {
    pub id: i64,
    pub username: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: UserIdentity,
    pub exp: usize,
    #[serde(default)]
    pub token_type: String,
}

/// hash a password using pbkdf2-sha256
pub fn hash_password(password: &str) -> Result<String> {
    let config = UserConfig::load()?;
    let salt = config.server_id.as_bytes();

    let mut hash = [0u8; HASH_LENGTH];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ITERATIONS, &mut hash);

    Ok(hex::encode(hash))
}

/// verify a password against a hash using constant-time comparison
pub fn verify_password(password: &str, hash: &str) -> Result<bool> {
    let computed_hash = hash_password(password)?;
    let computed_bytes = computed_hash.as_bytes();
    let stored_bytes = hash.as_bytes();

    Ok(computed_bytes.ct_eq(stored_bytes).into())
}

/// generate a random string of the given length
pub fn generate_random_string(length: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

    let mut rng = rand::thread_rng();
    (0..length)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

/// create jwt token with token type and ttl seconds
pub fn create_jwt(
    identity: UserIdentity,
    secret: &str,
    token_type: &str,
    expires_in: u64,
) -> Result<String> {
    let expiration = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() + expires_in;

    let claims = Claims {
        sub: identity,
        exp: expiration as usize,
        token_type: token_type.to_string(),
    };

    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;

    Ok(token)
}

/// verify jwt token and optionally enforce token type
pub fn verify_jwt(token: &str, secret: &str, expected_type: Option<&str>) -> Result<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.sub = None;

    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    )?;

    let claims = token_data.claims;
    if let Some(t) = expected_type {
        if !claims.token_type.is_empty() && claims.token_type != t {
            return Err(anyhow::anyhow!("Invalid token type"));
        }
    }

    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_string() {
        let s1 = generate_random_string(32);
        let s2 = generate_random_string(32);

        assert_eq!(s1.len(), 32);
        assert_eq!(s2.len(), 32);
        assert_ne!(s1, s2); // Should be different (with very high probability)
    }
}
