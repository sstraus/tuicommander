//! RFC 8291 key derivation and the existing single-record aes128gcm encoding.
use ring::{
    aead::{AES_128_GCM, Aad, LessSafeKey, Nonce, UnboundKey},
    agreement::{self, ECDH_P256, EphemeralPrivateKey, UnparsedPublicKey},
    hkdf,
    rand::{SecureRandom, SystemRandom},
};

struct HkdfLength(usize);

impl hkdf::KeyType for HkdfLength {
    fn len(&self) -> usize {
        self.0
    }
}

pub(crate) fn hkdf_sha256<const N: usize>(salt: &[u8], input: &[u8], info: &[u8]) -> [u8; N] {
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, salt).extract(input);
    let mut output = [0; N];
    prk.expand(&[info], HkdfLength(N))
        .expect("fixed SHA-256 HKDF output fits the RFC5869 limit")
        .fill(&mut output)
        .expect("HKDF output buffer matches requested length");
    output
}

pub(super) fn push_key_material(
    auth: &[u8],
    shared: &[u8],
    ua_public: &[u8],
    as_public: &[u8],
    salt: &[u8],
) -> ([u8; 32], [u8; 16], [u8; 12]) {
    let mut info = b"WebPush: info\0".to_vec();
    info.extend_from_slice(ua_public);
    info.extend_from_slice(as_public);
    let ikm = hkdf_sha256(auth, shared, &info);
    let key = hkdf_sha256(salt, &ikm, b"Content-Encoding: aes128gcm\0");
    let nonce = hkdf_sha256(salt, &ikm, b"Content-Encoding: nonce\0");
    (ikm, key, nonce)
}

pub(super) fn encrypt_push_record(
    salt: &[u8; 16],
    as_public: &[u8],
    key: &[u8; 16],
    nonce: &[u8; 12],
    message: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let size = u32::try_from(
        message
            .len()
            .checked_add(17)
            .ok_or_else(|| anyhow::anyhow!("record too large"))?,
    )?;
    let mut body = salt.to_vec();
    body.extend_from_slice(&size.to_be_bytes());
    body.push(u8::try_from(as_public.len())?);
    body.extend_from_slice(as_public);
    let mut encrypted = message.to_vec();
    encrypted.push(2);
    let cipher = LessSafeKey::new(
        UnboundKey::new(&AES_128_GCM, key).map_err(|_| anyhow::anyhow!("invalid push key"))?,
    );
    cipher
        .seal_in_place_append_tag(
            Nonce::assume_unique_for_key(*nonce),
            Aad::empty(),
            &mut encrypted,
        )
        .map_err(|_| anyhow::anyhow!("push encryption failed"))?;
    body.extend_from_slice(&encrypted);
    Ok(body)
}

pub(super) fn encrypt_push(
    ua_public: &[u8],
    auth: &[u8],
    message: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let rng = SystemRandom::new();
    let private = EphemeralPrivateKey::generate(&ECDH_P256, &rng)
        .map_err(|_| anyhow::anyhow!("push ECDH key generation failed"))?;
    let public = private
        .compute_public_key()
        .map_err(|_| anyhow::anyhow!("push public key generation failed"))?;
    let mut salt = [0; 16];
    rng.fill(&mut salt)
        .map_err(|_| anyhow::anyhow!("push salt generation failed"))?;
    agreement::agree_ephemeral(
        private,
        &UnparsedPublicKey::new(&ECDH_P256, ua_public),
        |shared| {
            let (_, key, nonce) =
                push_key_material(auth, shared, ua_public, public.as_ref(), &salt);
            encrypt_push_record(&salt, public.as_ref(), &key, &nonce, message)
        },
    )
    .map_err(|_| anyhow::anyhow!("invalid push ECDH public key"))?
}
