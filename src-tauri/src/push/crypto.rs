//! Fixed-material seam for guarding the existing Web Push record encoding.
use aes_gcm::{aead::Aead, Aes128Gcm, KeyInit};
use hkdf::Hkdf;
use sha2::Sha256;

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
    let mut ikm = [0; 32];
    Hkdf::<Sha256>::new(Some(auth), shared)
        .expand(&info, &mut ikm)
        .unwrap();
    let hk = Hkdf::<Sha256>::new(Some(salt), &ikm);
    let mut key = [0; 16];
    let mut nonce = [0; 12];
    hk.expand(b"Content-Encoding: aes128gcm\0", &mut key)
        .unwrap();
    hk.expand(b"Content-Encoding: nonce\0", &mut nonce).unwrap();
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
    let mut plaintext = message.to_vec();
    plaintext.push(2);
    let encrypted = Aes128Gcm::new(&(*key).into())
        .encrypt(&(*nonce).into(), plaintext.as_slice())
        .map_err(|_| anyhow::anyhow!("push encryption failed"))?;
    body.extend_from_slice(&encrypted);
    Ok(body)
}
