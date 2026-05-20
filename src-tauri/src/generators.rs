use base64::{Engine, engine::general_purpose::STANDARD};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GeneratorRequest {
    Password {
        length: u8,
        uppercase: bool,
        lowercase: bool,
        numbers: bool,
        symbols: bool,
    },
    UuidV4,
    UuidV7,
    Ulid,
    Cuid2,
    JwtSecret,
    TotpSecret,
    NanoId {
        length: u8,
    },
    Slug,
    Ed25519Keypair,
}

#[derive(Debug, Serialize)]
pub struct GeneratorResult {
    pub value: String,
    pub extra: Option<String>,
}

#[tauri::command]
pub fn generate_value(request: GeneratorRequest) -> Result<GeneratorResult, String> {
    // SECURITY: do not log generated value
    match request {
        GeneratorRequest::Ed25519Keypair => gen_ed25519_keypair(),
        other => {
            let value = Zeroizing::new(match other {
                GeneratorRequest::Password {
                    length,
                    uppercase,
                    lowercase,
                    numbers,
                    symbols,
                } => gen_password(length, uppercase, lowercase, numbers, symbols)?,
                GeneratorRequest::UuidV4 => uuid::Uuid::new_v4().to_string(),
                GeneratorRequest::UuidV7 => uuid::Uuid::now_v7().to_string(),
                GeneratorRequest::Ulid => ulid::Ulid::new().to_string(),
                GeneratorRequest::Cuid2 => cuid2::create_id(),
                GeneratorRequest::JwtSecret => gen_hex_bytes(32),
                GeneratorRequest::TotpSecret => gen_base32_bytes(20),
                GeneratorRequest::NanoId { length } => gen_nanoid(length),
                GeneratorRequest::Slug => gen_slug(),
                GeneratorRequest::Ed25519Keypair => unreachable!(),
            });
            Ok(GeneratorResult {
                value: (*value).clone(),
                extra: None,
            })
        }
    }
}

fn gen_password(
    length: u8,
    uppercase: bool,
    lowercase: bool,
    numbers: bool,
    symbols: bool,
) -> Result<String, String> {
    let mut charset: Vec<u8> = Vec::new();
    if uppercase {
        charset.extend_from_slice(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    }
    if lowercase {
        charset.extend_from_slice(b"abcdefghijklmnopqrstuvwxyz");
    }
    if numbers {
        charset.extend_from_slice(b"0123456789");
    }
    if symbols {
        charset.extend_from_slice(b"!@#$%^&*()-_=+[]{}|;:,.<>?");
    }
    if charset.is_empty() {
        return Err("at least one character class must be enabled".into());
    }
    let len = length.clamp(4, 128) as usize;
    let mut rng = OsRng;
    let pw: String = (0..len)
        .map(|_| charset[random_idx(&mut rng, charset.len())] as char)
        .collect();
    Ok(pw)
}

fn gen_hex_bytes(n: usize) -> String {
    let mut buf = Zeroizing::new(vec![0u8; n]);
    OsRng.fill_bytes(&mut buf);
    hex::encode(&*buf)
}

fn gen_base32_bytes(n: usize) -> String {
    let mut buf = Zeroizing::new(vec![0u8; n]);
    OsRng.fill_bytes(&mut buf);
    base32::encode(base32::Alphabet::RFC4648 { padding: false }, &buf)
}

fn gen_nanoid(length: u8) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
    let len = length.clamp(4, 64) as usize;
    let mut rng = OsRng;
    (0..len)
        .map(|_| ALPHABET[random_idx(&mut rng, ALPHABET.len())] as char)
        .collect()
}

fn gen_slug() -> String {
    #[rustfmt::skip]
    const ADJ: &[&str] = &[
        "brave","calm","dark","eager","fast","glad","huge","icy","jolly","kind",
        "lazy","misty","noble","odd","proud","quiet","rapid","silent","tiny","unique",
        "vast","wild","young","zealous","ancient","bold","crisp","daring","elegant","fierce",
        "gentle","hollow","ivory","jade","keen","lofty","mellow","nimble","open","pale",
        "quirky","rustic","sharp","swift","tall","urban","vivid","warm","exact","zesty",
    ];
    #[rustfmt::skip]
    const NOUN: &[&str] = &[
        "anchor","bridge","comet","drift","ember","flame","grove","haven","isle","jade",
        "knoll","lagoon","mesa","nexus","orbit","peak","quartz","ridge","shore","tide",
        "umbra","vale","wave","xenon","yard","zenith","atlas","basin","cedar","delta",
        "epoch","fjord","glyph","haze","inlet","junction","kite","lens","manor","nova",
        "oasis","prism","quest","realm","shard","tower","umber","vertex","wisp","apex",
    ];
    let mut rng = OsRng;
    let adj = ADJ[random_idx(&mut rng, ADJ.len())];
    let noun = NOUN[random_idx(&mut rng, NOUN.len())];
    let n = (rng.next_u32() & 0xFFFF) as u16;
    format!("{adj}-{noun}-{n:04}")
}

/// Unbiased index in [0, len) using rejection sampling — avoids modulo bias.
fn random_idx(rng: &mut OsRng, len: usize) -> usize {
    assert!(len > 0 && u32::try_from(len).is_ok());
    let len32 = len as u32;
    let threshold = u32::MAX - (u32::MAX % len32);
    loop {
        let v = rng.next_u32();
        if v <= threshold {
            return (v % len32) as usize;
        }
    }
}

fn gen_ed25519_keypair() -> Result<GeneratorResult, String> {
    use ring::signature::{Ed25519KeyPair, KeyPair};

    let rng = ring::rand::SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).map_err(|e| e.to_string())?;
    let pair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).map_err(|e| e.to_string())?;

    // PKCS#8 PEM — NOT OpenSSH format. For SSH keys use: ssh-keygen -t ed25519
    let priv_pem = format!(
        "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
        STANDARD.encode(pkcs8.as_ref())
    );
    let pub_pem = format!(
        "-----BEGIN PUBLIC KEY-----\n{}\n-----END PUBLIC KEY-----",
        STANDARD.encode(pair.public_key().as_ref())
    );
    Ok(GeneratorResult {
        value: priv_pem,
        extra: Some(pub_pem),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn genv(req: GeneratorRequest) -> GeneratorResult {
        generate_value(req).expect("generate_value failed")
    }

    #[test]
    fn password_default_charset() {
        let r = genv(GeneratorRequest::Password {
            length: 32,
            uppercase: true,
            lowercase: true,
            numbers: true,
            symbols: true,
        });
        assert_eq!(r.value.len(), 32);
        assert!(r.value.chars().all(|c| c.is_ascii()));
    }

    #[test]
    fn password_lowercase_only() {
        let r = genv(GeneratorRequest::Password {
            length: 20,
            uppercase: false,
            lowercase: true,
            numbers: false,
            symbols: false,
        });
        assert!(r.value.chars().all(|c| c.is_ascii_lowercase()));
    }

    #[test]
    fn password_empty_charset_errors() {
        let err = generate_value(GeneratorRequest::Password {
            length: 16,
            uppercase: false,
            lowercase: false,
            numbers: false,
            symbols: false,
        });
        assert!(err.is_err());
    }

    #[test]
    fn uuid_v4_format() {
        let r = genv(GeneratorRequest::UuidV4);
        // xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
        assert_eq!(r.value.len(), 36);
        assert_eq!(r.value.chars().nth(14).unwrap(), '4');
    }

    #[test]
    fn uuid_v7_format() {
        let r = genv(GeneratorRequest::UuidV7);
        assert_eq!(r.value.len(), 36);
        // version nibble at position 14 is '7'
        assert_eq!(r.value.chars().nth(14).unwrap(), '7');
    }

    #[test]
    fn ulid_format() {
        let r = genv(GeneratorRequest::Ulid);
        assert_eq!(r.value.len(), 26);
        assert!(
            r.value
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        );
    }

    #[test]
    fn cuid2_non_empty() {
        let r = genv(GeneratorRequest::Cuid2);
        assert!(!r.value.is_empty());
    }

    #[test]
    fn jwt_secret_is_64_hex_chars() {
        let r = genv(GeneratorRequest::JwtSecret);
        assert_eq!(r.value.len(), 64);
        assert!(r.value.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn totp_secret_is_valid_base32() {
        let r = genv(GeneratorRequest::TotpSecret);
        // RFC 4648 base32: A-Z + 2-7, no padding
        assert!(!r.value.is_empty());
        assert!(r.value.chars().all(|c| matches!(c, 'A'..='Z' | '2'..='7')));
    }

    #[test]
    fn nanoid_length() {
        let r = genv(GeneratorRequest::NanoId { length: 21 });
        assert_eq!(r.value.len(), 21);
    }

    #[test]
    fn slug_format() {
        let r = genv(GeneratorRequest::Slug);
        let parts: Vec<&str> = r.value.split('-').collect();
        assert!(parts.len() >= 3);
    }

    #[test]
    fn ed25519_pem_headers() {
        let r = genv(GeneratorRequest::Ed25519Keypair);
        assert!(r.value.contains("-----BEGIN PRIVATE KEY-----"));
        assert!(r.value.contains("-----END PRIVATE KEY-----"));
        let pub_key = r.extra.expect("extra should have public key");
        assert!(pub_key.contains("-----BEGIN PUBLIC KEY-----"));
        assert!(pub_key.contains("-----END PUBLIC KEY-----"));
    }
}
