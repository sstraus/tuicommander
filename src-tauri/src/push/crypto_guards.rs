//! Migration guards. RFC 8291 section 5 / appendix A supplies the push vectors.
use super::*;
use p256::elliptic_curve::sec1::ToEncodedPoint;

const UA_PRIVATE: &str = "q1dXpw3UpT5VOmu_cf_v6ih07Aems3njxI-JWgLcM94";
const UA_PUBLIC: &str =
    "BCVxsr7N_eNgVRqvHtD0zTZsEc6-VV-JvLexhqUzORcxaOzi6-AYWXvTBHm4bjyPjs7Vd8pZGH6SRpkNtoIAiw4";
const AS_PRIVATE: &str = "yfWPiYE-n46HLnH0KqZOF1fJJU3MYrct3AELtAQ-oRw";
const AS_PUBLIC: &str =
    "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8";
const AUTH: &str = "BTBZMqHH6r4Tts7J_aSIgg";
const SALT: &str = "DGv6ra1nlYgDCS1FRnbzlw";
const MESSAGE: &[u8] = b"When I grow up, I want to be a watermelon";

fn decode(value: &str) -> Vec<u8> {
    Base64UrlUnpadded::decode_vec(value).unwrap()
}

#[test]
fn crypto_guard_rfc8291_hkdf_and_ciphertext_prevent_label_or_padding_drift() {
    let sender = p256::SecretKey::from_slice(&decode(AS_PRIVATE)).unwrap();
    let receiver = p256::PublicKey::from_sec1_bytes(&decode(UA_PUBLIC)).unwrap();
    let shared = p256::ecdh::diffie_hellman(sender.to_nonzero_scalar(), receiver.as_affine());
    assert_eq!(
        shared.raw_secret_bytes().as_slice(),
        decode("kyrL1jIIOHEzg3sM2ZWRHDRB62YACZhhSlknJ672kSs")
    );
    assert_eq!(
        sender.public_key().to_encoded_point(false).as_bytes(),
        decode(AS_PUBLIC)
    );
    let (ikm, key, nonce) = push_key_material(
        &decode(AUTH),
        shared.raw_secret_bytes(),
        &decode(UA_PUBLIC),
        &decode(AS_PUBLIC),
        &decode(SALT),
    );
    assert_eq!(
        ikm.as_slice(),
        decode("S4lYMb_L0FxCeq0WhDx813KgSYqU26kOyzWUdsXYyrg")
    );
    assert_eq!(key.as_slice(), decode("oIhVW04MRdy2XN9CiKLxTg"));
    assert_eq!(nonce.as_slice(), decode("4h_95klXJ5E_qnoN"));
    let record = encrypt_push_record(
        &decode(SALT).try_into().unwrap(),
        &decode(AS_PUBLIC),
        &key,
        &nonce,
        MESSAGE,
    )
    .unwrap();
    // The old library advertises the exact record length, rather than RFC's 4096.
    assert_eq!(&record[16..20], &58u32.to_be_bytes());
    assert_eq!(&record[21..86], decode(AS_PUBLIC));
    assert_eq!(
        &record[86..],
        decode("8pfeW0KbunFT06SuDKoJH9Ql87S1QUrdirN6GcG7sFz1y1sqLgVi1VhjVkHsUoEsbI_0LpXMuGvnzQ")
    );
}

#[test]
fn crypto_guard_real_push_request_prevents_ecdh_order_and_header_drift() {
    let receiver = p256::SecretKey::from_slice(&decode(UA_PRIVATE)).unwrap();
    let signing_key = p256::ecdsa::SigningKey::from_slice(&decode(AS_PRIVATE)).unwrap();
    let sub = PushSubscription {
        endpoint: "https://fcm.googleapis.com/fcm/send/guard".into(),
        keys: PushSubscriptionKeys {
            p256dh: UA_PUBLIC.into(),
            auth: AUTH.into(),
        },
        created_at: chrono::Utc::now(),
    };
    // Includes empty and long records: old web-push-native emits one record without padding.
    for message in [Vec::new(), MESSAGE.to_vec(), vec![0x61; 5000]] {
        let (_, request) =
            build_push_request(&sub, &signing_key, "mailto:test@example.com", &message).unwrap();
        assert_eq!(request.method(), "POST");
        assert_eq!(request.uri().to_string(), sub.endpoint);
        assert_eq!(request.headers()["ttl"], "43200");
        assert_eq!(request.headers()["content-encoding"], "aes128gcm");
        assert_eq!(
            request.headers()["content-type"],
            "application/octet-stream"
        );
        let body = request.body();
        assert_eq!(request.headers()["content-length"], body.len().to_string());
        assert_eq!(body[20], 65);
        let sender = p256::PublicKey::from_sec1_bytes(&body[21..86]).unwrap();
        let shared = p256::ecdh::diffie_hellman(receiver.to_nonzero_scalar(), sender.as_affine());
        let (_, key, nonce) = push_key_material(
            &decode(AUTH),
            shared.raw_secret_bytes(),
            &decode(UA_PUBLIC),
            &body[21..86],
            &body[..16],
        );
        let expected = encrypt_push_record(
            &body[..16].try_into().unwrap(),
            &body[21..86],
            &key,
            &nonce,
            &message,
        )
        .unwrap();
        assert_eq!(
            *body, expected,
            "real sender must match the fixed-material encoding"
        );
        let key = ring::aead::LessSafeKey::new(
            ring::aead::UnboundKey::new(&ring::aead::AES_128_GCM, &key).unwrap(),
        );
        let mut encrypted = body[86..].to_vec();
        let decrypted = key
            .open_in_place(
                ring::aead::Nonce::assume_unique_for_key(nonce),
                ring::aead::Aad::empty(),
                &mut encrypted,
            )
            .unwrap();
        assert_eq!(decrypted, [message.as_slice(), &[2]].concat());
    }
}

#[test]
fn crypto_guard_stored_scalar_vapid_verifies_independently_and_rejects_tampering() {
    use p256::ecdsa::signature::Verifier;
    let signing_key = p256::ecdsa::SigningKey::from_slice(&decode(AS_PRIVATE)).unwrap();
    let endpoint = "https://fcm.googleapis.com/fcm/send/guard".parse().unwrap();
    let header =
        build_vapid_authorization(&signing_key, &endpoint, "mailto:test@example.com", 3600)
            .unwrap();
    let (jwt, public) = header
        .to_str()
        .unwrap()
        .strip_prefix("vapid t=")
        .unwrap()
        .split_once(", k=")
        .unwrap();
    assert_eq!(public, AS_PUBLIC);
    let (input, signature) = jwt.rsplit_once('.').unwrap();
    let signature = decode(signature);
    let verifier = ring::signature::UnparsedPublicKey::new(
        &ring::signature::ECDSA_P256_SHA256_FIXED,
        decode(public),
    );
    verifier.verify(input.as_bytes(), &signature).unwrap();
    signing_key
        .verifying_key()
        .verify(
            input.as_bytes(),
            &p256::ecdsa::Signature::from_slice(&signature).unwrap(),
        )
        .unwrap();
    assert!(verifier.verify(b"tampered", &signature).is_err());
}

#[test]
fn crypto_guard_ring_cannot_recover_public_key_from_stored_scalar_alone() {
    let result = ring::signature::EcdsaKeyPair::from_private_key_and_public_key(
        &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
        &decode(AS_PRIVATE),
        &[],
        &ring::rand::SystemRandom::new(),
    );
    let error = result
        .err()
        .expect("ring requires the public key alongside the stored scalar");
    assert_eq!(
        format!("{error:?}"),
        "KeyRejected(\"InconsistentComponents\")"
    );
}
