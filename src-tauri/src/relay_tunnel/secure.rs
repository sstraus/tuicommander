//! Noise PSK channel between the desktop and the daemon.
//!
//! `Noise_NNpsk0_25519_ChaChaPoly_SHA256`: the pre-shared key P authenticates
//! both sides, fresh ephemeral X25519 keys give every attachment its own
//! session keys, and each direction has its own key and a strictly increasing
//! nonce. A dropped, replayed, reordered or reflected record therefore fails
//! to open, and the channel refuses every later record (the caller closes it).
//!
//! The relay holds the bearer R and the room, never P, so nothing it sees can
//! derive these keys. The prologue binds protocol version, pairing id and
//! room; a mismatch in any of them fails the first message. The relay hostname
//! is deliberately not bound: the two ends may reach one relay under different
//! names.

use std::fmt;

use ring::rand::{SecureRandom, SystemRandom};
use snow::{Builder, HandshakeState, TransportState};
use zeroize::Zeroizing;

/// Wire and mux protocol version, bound into the prologue.
pub(crate) const PROTOCOL_VERSION: u8 = 1;
const NOISE_PARAMS: &str = "Noise_NNpsk0_25519_ChaChaPoly_SHA256";
const PROLOGUE_TAG: &[u8] = b"tuic-relay-tunnel";
const TAG_LEN: usize = 16;
/// Largest plaintext of one record: a 16 KiB data chunk plus frame header.
pub(crate) const MAX_RECORD_PLAINTEXT: usize = 17 * 1024;
const MAX_HANDSHAKE_MESSAGE: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SecureError {
    /// The handshake message did not verify: wrong secret or binding, or garbage.
    Handshake,
    /// A record failed authentication, or arrived replayed, dropped or reordered.
    Record,
    /// The input exceeds the record or handshake size limit.
    TooLarge,
    /// An earlier failure made this channel unusable.
    Broken,
    Internal(String),
}

impl fmt::Display for SecureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Handshake => write!(f, "relay tunnel handshake rejected"),
            Self::Record => write!(f, "relay tunnel record rejected"),
            Self::TooLarge => write!(f, "relay tunnel message too large"),
            Self::Broken => write!(f, "relay tunnel channel is closed"),
            Self::Internal(e) => write!(f, "relay tunnel crypto error: {e}"),
        }
    }
}

impl std::error::Error for SecureError {}

fn internal(e: snow::Error) -> SecureError {
    SecureError::Internal(e.to_string())
}

/// The 32-byte pairing secret P. Zeroed on drop, never printed.
#[derive(Clone)]
pub(crate) struct PairingSecret(Zeroizing<[u8; 32]>);

impl PairingSecret {
    pub(crate) fn generate() -> Result<Self, SecureError> {
        let mut bytes = Zeroizing::new([0u8; 32]);
        SystemRandom::new()
            .fill(bytes.as_mut())
            .map_err(|_| SecureError::Internal("OS random source failed".into()))?;
        Ok(Self(bytes))
    }

    pub(crate) fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    pub(crate) fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for PairingSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingSecret(<redacted>)")
    }
}

/// What the handshake is bound to besides the secret.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ChannelBinding<'a> {
    pub pairing_id: &'a str,
    pub room: &'a str,
}

/// Every field is length-prefixed so no two bindings share an encoding.
fn prologue(binding: &ChannelBinding<'_>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(PROLOGUE_TAG);
    out.push(PROTOCOL_VERSION);
    for field in [binding.pairing_id, binding.room] {
        out.extend_from_slice(&(field.len() as u32).to_be_bytes());
        out.extend_from_slice(field.as_bytes());
    }
    out
}

/// One attachment's handshake. The desktop is the initiator and writes first.
pub(crate) struct Handshake {
    state: HandshakeState,
}

impl Handshake {
    pub(crate) fn initiator(
        secret: &PairingSecret,
        binding: &ChannelBinding<'_>,
    ) -> Result<Self, SecureError> {
        Self::build(secret, binding, true)
    }

    pub(crate) fn responder(
        secret: &PairingSecret,
        binding: &ChannelBinding<'_>,
    ) -> Result<Self, SecureError> {
        Self::build(secret, binding, false)
    }

    fn build(
        secret: &PairingSecret,
        binding: &ChannelBinding<'_>,
        initiator: bool,
    ) -> Result<Self, SecureError> {
        let params = NOISE_PARAMS
            .parse()
            .map_err(|e: snow::Error| SecureError::Internal(e.to_string()))?;
        let prologue = prologue(binding);
        let builder = Builder::new(params)
            .psk(0, secret.as_bytes())
            .map_err(internal)?
            .prologue(&prologue)
            .map_err(internal)?;
        let state = if initiator {
            builder.build_initiator()
        } else {
            builder.build_responder()
        }
        .map_err(internal)?;
        Ok(Self { state })
    }

    /// The next outbound handshake message.
    pub(crate) fn write(&mut self) -> Result<Vec<u8>, SecureError> {
        let mut buf = vec![0u8; MAX_HANDSHAKE_MESSAGE];
        let n = self.state.write_message(&[], &mut buf).map_err(internal)?;
        buf.truncate(n);
        Ok(buf)
    }

    /// Consume the peer's handshake message; `Handshake` means it did not verify.
    pub(crate) fn read(&mut self, message: &[u8]) -> Result<(), SecureError> {
        if message.len() > MAX_HANDSHAKE_MESSAGE {
            return Err(SecureError::TooLarge);
        }
        let mut payload = vec![0u8; MAX_HANDSHAKE_MESSAGE];
        match self.state.read_message(message, &mut payload) {
            Ok(0) => Ok(()),
            _ => Err(SecureError::Handshake),
        }
    }

    pub(crate) fn finish(self) -> Result<SecureChannel, SecureError> {
        if !self.state.is_handshake_finished() {
            return Err(SecureError::Handshake);
        }
        let transport = self.state.into_transport_mode().map_err(internal)?;
        Ok(SecureChannel {
            transport,
            broken: false,
        })
    }
}

/// Ordered, authenticated records in both directions.
pub(crate) struct SecureChannel {
    transport: TransportState,
    broken: bool,
}

impl SecureChannel {
    pub(crate) fn seal(&mut self, plaintext: &[u8]) -> Result<Vec<u8>, SecureError> {
        if self.broken {
            return Err(SecureError::Broken);
        }
        if plaintext.len() > MAX_RECORD_PLAINTEXT {
            return Err(SecureError::TooLarge);
        }
        let mut out = vec![0u8; plaintext.len() + TAG_LEN];
        match self.transport.write_message(plaintext, &mut out) {
            Ok(n) => {
                out.truncate(n);
                Ok(out)
            }
            Err(e) => {
                self.broken = true;
                Err(internal(e))
            }
        }
    }

    /// Any failure poisons the channel: the stream has a hole or a forger.
    pub(crate) fn open(&mut self, record: &[u8]) -> Result<Vec<u8>, SecureError> {
        if self.broken {
            return Err(SecureError::Broken);
        }
        if record.len() > MAX_RECORD_PLAINTEXT + TAG_LEN {
            self.broken = true;
            return Err(SecureError::TooLarge);
        }
        let mut out = vec![0u8; record.len()];
        match self.transport.read_message(record, &mut out) {
            Ok(n) => {
                out.truncate(n);
                Ok(out)
            }
            Err(_) => {
                self.broken = true;
                Err(SecureError::Record)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding() -> ChannelBinding<'static> {
        ChannelBinding {
            pairing_id: "pair-1",
            room: "room-1",
        }
    }

    fn pair_with(
        secret_i: &PairingSecret,
        bind_i: &ChannelBinding<'_>,
        secret_r: &PairingSecret,
        bind_r: &ChannelBinding<'_>,
    ) -> Result<(SecureChannel, SecureChannel), SecureError> {
        let mut i = Handshake::initiator(secret_i, bind_i)?;
        let mut r = Handshake::responder(secret_r, bind_r)?;
        r.read(&i.write()?)?;
        i.read(&r.write()?)?;
        Ok((i.finish()?, r.finish()?))
    }

    fn pair() -> (SecureChannel, SecureChannel) {
        let p = PairingSecret::generate().unwrap();
        pair_with(&p, &binding(), &p, &binding()).unwrap()
    }

    #[test]
    fn records_flow_in_both_directions() {
        let (mut desktop, mut daemon) = pair();
        let a = desktop.seal(b"GET /sessions").unwrap();
        assert_eq!(daemon.open(&a).unwrap(), b"GET /sessions");
        let b = daemon.seal(b"200 []").unwrap();
        assert_eq!(desktop.open(&b).unwrap(), b"200 []");
    }

    // Catches a responder that accepts an initiator holding a different P.
    #[test]
    fn a_wrong_pairing_secret_fails_the_first_handshake_message() {
        let good = PairingSecret::generate().unwrap();
        let bad = PairingSecret::generate().unwrap();
        let mut i = Handshake::initiator(&bad, &binding()).unwrap();
        let mut r = Handshake::responder(&good, &binding()).unwrap();
        assert_eq!(r.read(&i.write().unwrap()), Err(SecureError::Handshake));
    }

    // Catches a prologue that forgets one field, or encodes two fields
    // ambiguously: moving a byte between pairing id and room must still fail.
    #[test]
    fn every_binding_field_is_authenticated() {
        let p = PairingSecret::generate().unwrap();
        let base = binding();
        let variants = [
            ChannelBinding {
                pairing_id: "pair-2",
                ..base
            },
            ChannelBinding {
                room: "room-2",
                ..base
            },
            ChannelBinding {
                pairing_id: "pair-1r",
                room: "oom-1",
            },
        ];
        for other in variants {
            assert!(
                pair_with(&p, &base, &p, &other).is_err(),
                "binding {other:?} was accepted"
            );
        }
    }

    // Catches a replayed record being accepted a second time.
    #[test]
    fn a_replayed_record_is_rejected() {
        let (mut desktop, mut daemon) = pair();
        let rec = desktop.seal(b"POST /sessions").unwrap();
        assert!(daemon.open(&rec).is_ok());
        assert_eq!(daemon.open(&rec), Err(SecureError::Record));
    }

    // Catches a relay that drops or reorders records going unnoticed.
    #[test]
    fn a_dropped_or_reordered_record_is_rejected() {
        let (mut desktop, mut daemon) = pair();
        let first = desktop.seal(b"one").unwrap();
        let second = desktop.seal(b"two").unwrap();
        assert_eq!(daemon.open(&second), Err(SecureError::Record));
        // The stream had a hole: even the record that was in order is refused.
        assert_eq!(daemon.open(&first), Err(SecureError::Broken));
    }

    // Catches a shared send/receive key: a record echoed back to its sender
    // must not open.
    #[test]
    fn a_reflected_record_is_rejected() {
        let (mut desktop, _daemon) = pair();
        let rec = desktop.seal(b"hello").unwrap();
        assert_eq!(desktop.open(&rec), Err(SecureError::Record));
    }

    // Catches reuse of session keys across attachments: a record of an old
    // epoch must not open in a new one, even when the relay replays the old
    // handshake message to start it.
    #[test]
    fn a_record_of_an_old_epoch_does_not_open_in_a_new_one() {
        let p = PairingSecret::generate().unwrap();
        let mut i1 = Handshake::initiator(&p, &binding()).unwrap();
        let mut r1 = Handshake::responder(&p, &binding()).unwrap();
        let msg1 = i1.write().unwrap();
        r1.read(&msg1).unwrap();
        i1.read(&r1.write().unwrap()).unwrap();
        let mut old_desktop = i1.finish().unwrap();
        let old_record = old_desktop.seal(b"POST /sessions").unwrap();

        let mut r2 = Handshake::responder(&p, &binding()).unwrap();
        r2.read(&msg1).unwrap(); // replayed first message
        r2.write().unwrap();
        let mut new_daemon = r2.finish().unwrap();
        assert_eq!(new_daemon.open(&old_record), Err(SecureError::Record));
    }

    // Catches a channel that keeps working after a forged record.
    #[test]
    fn a_failure_poisons_the_channel() {
        let (mut desktop, mut daemon) = pair();
        assert!(daemon.open(&[0u8; 40]).is_err());
        let good = desktop.seal(b"ok").unwrap();
        assert_eq!(daemon.open(&good), Err(SecureError::Broken));
        assert_eq!(daemon.seal(b"x"), Err(SecureError::Broken));
    }

    // Catches unbounded allocation from a peer-chosen length.
    #[test]
    fn oversized_input_is_refused_before_any_crypto() {
        let (mut desktop, mut daemon) = pair();
        assert_eq!(
            desktop.seal(&vec![0u8; MAX_RECORD_PLAINTEXT + 1]),
            Err(SecureError::TooLarge)
        );
        assert_eq!(
            daemon.open(&vec![0u8; MAX_RECORD_PLAINTEXT + TAG_LEN + 1]),
            Err(SecureError::TooLarge)
        );
        let p = PairingSecret::generate().unwrap();
        let mut r = Handshake::responder(&p, &binding()).unwrap();
        assert_eq!(r.read(&vec![0u8; 4096]), Err(SecureError::TooLarge));
        let full = vec![1u8; MAX_RECORD_PLAINTEXT];
        let (mut a, mut b) = pair();
        assert_eq!(b.open(&a.seal(&full).unwrap()).unwrap(), full);
    }

    // Catches the secret or the plaintext appearing in what the relay forwards.
    #[test]
    fn wire_bytes_contain_neither_the_secret_nor_the_plaintext() {
        let p = PairingSecret::generate().unwrap();
        let mut i = Handshake::initiator(&p, &binding()).unwrap();
        let mut r = Handshake::responder(&p, &binding()).unwrap();
        let m1 = i.write().unwrap();
        r.read(&m1).unwrap();
        let m2 = r.write().unwrap();
        i.read(&m2).unwrap();
        let mut desktop = i.finish().unwrap();
        let rec = desktop.seal(b"super-secret-command").unwrap();
        for wire in [&m1, &m2, &rec] {
            assert!(!wire.windows(32).any(|w| w == p.as_bytes()));
            assert!(!wire.windows(20).any(|w| w == b"super-secret-command"));
        }
    }

    // Catches P printed through `{:?}` in a log line.
    #[test]
    fn the_secret_never_prints() {
        let p = PairingSecret::from_bytes([0xAB; 32]);
        let shown = format!("{p:?}");
        assert!(!shown.contains("ab") && !shown.contains("AB") && shown.contains("redacted"));
    }
}
