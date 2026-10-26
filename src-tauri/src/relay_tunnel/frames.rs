//! The tunnel's transport vocabulary. One frame is the plaintext of one
//! `SecureChannel` record. There is no application command here: HTTP, SSE and
//! WebSocket bytes pass through `Data` unchanged.
//!
//! Wire: `type:u8 | stream:u32 BE | body`. Connection-level frames use stream 0.

use std::fmt;

/// A data chunk; `secure::MAX_RECORD_PLAINTEXT` leaves room for the header.
pub(crate) const MAX_DATA_CHUNK: usize = 16 * 1024;
/// The daemon API credential D is a token, not a blob.
pub(crate) const MAX_CREDENTIAL_LEN: usize = 1024;
const HEADER_LEN: usize = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenFailure {
    /// The daemon's own listener refused the connection.
    UpstreamUnreachable = 1,
    /// The daemon is at its concurrent-stream limit.
    TooManyStreams = 2,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Frame {
    Open {
        stream: u32,
    },
    Opened {
        stream: u32,
    },
    OpenFailed {
        stream: u32,
        reason: OpenFailure,
    },
    Data {
        stream: u32,
        bytes: Vec<u8>,
    },
    /// Receive credit: the sender may send this many more bytes on `stream`.
    WindowUpdate {
        stream: u32,
        credit: u32,
    },
    /// The sender finished writing; the other direction stays open.
    Fin {
        stream: u32,
    },
    /// Abort both directions.
    Reset {
        stream: u32,
    },
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    /// The daemon API credential D, sent once per attachment, only inside the
    /// authenticated channel. It grants nothing beyond what D grants over HTTP.
    Credential {
        bytes: Vec<u8>,
    },
}

impl fmt::Debug for Frame {
    // Never print Data or Credential bodies: they carry terminal output and D.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Data { stream, bytes } => {
                write!(f, "Data{{stream:{stream},len:{}}}", bytes.len())
            }
            Self::Credential { bytes } => write!(f, "Credential{{len:{}}}", bytes.len()),
            Self::Open { stream } => write!(f, "Open{{{stream}}}"),
            Self::Opened { stream } => write!(f, "Opened{{{stream}}}"),
            Self::OpenFailed { stream, reason } => write!(f, "OpenFailed{{{stream},{reason:?}}}"),
            Self::WindowUpdate { stream, credit } => write!(f, "WindowUpdate{{{stream},{credit}}}"),
            Self::Fin { stream } => write!(f, "Fin{{{stream}}}"),
            Self::Reset { stream } => write!(f, "Reset{{{stream}}}"),
            Self::Ping { nonce } => write!(f, "Ping{{{nonce}}}"),
            Self::Pong { nonce } => write!(f, "Pong{{{nonce}}}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FrameError {
    Truncated,
    UnknownType(u8),
    TooLarge,
    /// A stream frame on stream 0, or a connection frame on a stream.
    BadStream,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid relay tunnel frame: {self:?}")
    }
}

impl std::error::Error for FrameError {}

mod kind {
    pub(super) const OPEN: u8 = 1;
    pub(super) const OPENED: u8 = 2;
    pub(super) const OPEN_FAILED: u8 = 3;
    pub(super) const DATA: u8 = 4;
    pub(super) const WINDOW_UPDATE: u8 = 5;
    pub(super) const FIN: u8 = 6;
    pub(super) const RESET: u8 = 7;
    pub(super) const PING: u8 = 8;
    pub(super) const PONG: u8 = 9;
    pub(super) const CREDENTIAL: u8 = 10;
}

fn put(kind: u8, stream: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.push(kind);
    out.extend_from_slice(&stream.to_be_bytes());
    out.extend_from_slice(body);
    out
}

impl Frame {
    /// Encode, refusing a body larger than its limit so a bug in the caller
    /// cannot build a record the peer would reject.
    pub(crate) fn encode(&self) -> Result<Vec<u8>, FrameError> {
        Ok(match self {
            Self::Open { stream } => put(kind::OPEN, *stream, &[]),
            Self::Opened { stream } => put(kind::OPENED, *stream, &[]),
            Self::OpenFailed { stream, reason } => {
                put(kind::OPEN_FAILED, *stream, &[*reason as u8])
            }
            Self::Data { stream, bytes } => {
                if bytes.len() > MAX_DATA_CHUNK {
                    return Err(FrameError::TooLarge);
                }
                put(kind::DATA, *stream, bytes)
            }
            Self::WindowUpdate { stream, credit } => {
                put(kind::WINDOW_UPDATE, *stream, &credit.to_be_bytes())
            }
            Self::Fin { stream } => put(kind::FIN, *stream, &[]),
            Self::Reset { stream } => put(kind::RESET, *stream, &[]),
            Self::Ping { nonce } => put(kind::PING, 0, &nonce.to_be_bytes()),
            Self::Pong { nonce } => put(kind::PONG, 0, &nonce.to_be_bytes()),
            Self::Credential { bytes } => {
                if bytes.len() > MAX_CREDENTIAL_LEN {
                    return Err(FrameError::TooLarge);
                }
                put(kind::CREDENTIAL, 0, bytes)
            }
        })
    }

    pub(crate) fn decode(buf: &[u8]) -> Result<Self, FrameError> {
        if buf.len() < HEADER_LEN {
            return Err(FrameError::Truncated);
        }
        let stream = u32::from_be_bytes(buf[1..HEADER_LEN].try_into().expect("4 bytes"));
        let body = &buf[HEADER_LEN..];
        let needs_stream = |s: u32| {
            if s == 0 {
                Err(FrameError::BadStream)
            } else {
                Ok(s)
            }
        };
        let connection_level = |s: u32| {
            if s != 0 {
                Err(FrameError::BadStream)
            } else {
                Ok(())
            }
        };
        let empty = |b: &[u8]| {
            if b.is_empty() {
                Ok(())
            } else {
                Err(FrameError::TooLarge)
            }
        };
        let u32_body = |b: &[u8]| -> Result<u32, FrameError> {
            <[u8; 4]>::try_from(b)
                .map(u32::from_be_bytes)
                .map_err(|_| FrameError::Truncated)
        };
        let u64_body = |b: &[u8]| -> Result<u64, FrameError> {
            <[u8; 8]>::try_from(b)
                .map(u64::from_be_bytes)
                .map_err(|_| FrameError::Truncated)
        };
        Ok(match buf[0] {
            kind::OPEN => {
                empty(body)?;
                Self::Open {
                    stream: needs_stream(stream)?,
                }
            }
            kind::OPENED => {
                empty(body)?;
                Self::Opened {
                    stream: needs_stream(stream)?,
                }
            }
            kind::OPEN_FAILED => {
                let reason = match body {
                    [1] => OpenFailure::UpstreamUnreachable,
                    [2] => OpenFailure::TooManyStreams,
                    [] => return Err(FrameError::Truncated),
                    _ => return Err(FrameError::TooLarge),
                };
                Self::OpenFailed {
                    stream: needs_stream(stream)?,
                    reason,
                }
            }
            kind::DATA => {
                if body.len() > MAX_DATA_CHUNK {
                    return Err(FrameError::TooLarge);
                }
                Self::Data {
                    stream: needs_stream(stream)?,
                    bytes: body.to_vec(),
                }
            }
            kind::WINDOW_UPDATE => Self::WindowUpdate {
                stream: needs_stream(stream)?,
                credit: u32_body(body)?,
            },
            kind::FIN => {
                empty(body)?;
                Self::Fin {
                    stream: needs_stream(stream)?,
                }
            }
            kind::RESET => {
                empty(body)?;
                Self::Reset {
                    stream: needs_stream(stream)?,
                }
            }
            kind::PING => {
                connection_level(stream)?;
                Self::Ping {
                    nonce: u64_body(body)?,
                }
            }
            kind::PONG => {
                connection_level(stream)?;
                Self::Pong {
                    nonce: u64_body(body)?,
                }
            }
            kind::CREDENTIAL => {
                connection_level(stream)?;
                if body.len() > MAX_CREDENTIAL_LEN {
                    return Err(FrameError::TooLarge);
                }
                Self::Credential {
                    bytes: body.to_vec(),
                }
            }
            other => return Err(FrameError::UnknownType(other)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::relay_tunnel::secure::MAX_RECORD_PLAINTEXT;

    fn round_trip(frame: Frame) {
        assert_eq!(Frame::decode(&frame.encode().unwrap()).unwrap(), frame);
    }

    #[test]
    fn every_frame_round_trips() {
        round_trip(Frame::Open { stream: 1 });
        round_trip(Frame::Opened { stream: 1 });
        round_trip(Frame::OpenFailed {
            stream: 2,
            reason: OpenFailure::TooManyStreams,
        });
        round_trip(Frame::Data {
            stream: 3,
            bytes: vec![0, 1, 255],
        });
        round_trip(Frame::WindowUpdate {
            stream: 3,
            credit: 65_536,
        });
        round_trip(Frame::Fin { stream: 3 });
        round_trip(Frame::Reset { stream: 3 });
        round_trip(Frame::Ping { nonce: u64::MAX });
        round_trip(Frame::Pong { nonce: 7 });
        round_trip(Frame::Credential {
            bytes: b"token".to_vec(),
        });
    }

    // Catches a full data chunk not fitting in one secure record, which would
    // fail only under load when a PTY fills a chunk.
    #[test]
    fn a_full_data_frame_fits_one_secure_record() {
        let frame = Frame::Data {
            stream: 1,
            bytes: vec![9; MAX_DATA_CHUNK],
        };
        assert!(frame.encode().unwrap().len() <= MAX_RECORD_PLAINTEXT);
    }

    // Catches an unbounded allocation from a peer-chosen body.
    #[test]
    fn oversized_bodies_are_refused_in_both_directions() {
        let big = Frame::Data {
            stream: 1,
            bytes: vec![0; MAX_DATA_CHUNK + 1],
        };
        assert_eq!(big.encode(), Err(FrameError::TooLarge));
        let mut wire = put(kind::DATA, 1, &vec![0; MAX_DATA_CHUNK + 1]);
        assert_eq!(Frame::decode(&wire), Err(FrameError::TooLarge));
        wire = put(kind::CREDENTIAL, 0, &vec![0; MAX_CREDENTIAL_LEN + 1]);
        assert_eq!(Frame::decode(&wire), Err(FrameError::TooLarge));
    }

    // Catches stream 0 being used as a real stream, or a connection frame
    // smuggled onto one: stream 0 is reserved for the connection.
    #[test]
    fn stream_zero_is_reserved_for_connection_frames() {
        assert_eq!(
            Frame::decode(&put(kind::OPEN, 0, &[])),
            Err(FrameError::BadStream)
        );
        assert_eq!(
            Frame::decode(&put(kind::DATA, 0, b"x")),
            Err(FrameError::BadStream)
        );
        assert_eq!(
            Frame::decode(&put(kind::PING, 5, &1u64.to_be_bytes())),
            Err(FrameError::BadStream)
        );
        assert_eq!(
            Frame::decode(&put(kind::CREDENTIAL, 5, b"d")),
            Err(FrameError::BadStream)
        );
    }

    #[test]
    fn malformed_frames_are_errors_not_panics() {
        assert_eq!(Frame::decode(&[]), Err(FrameError::Truncated));
        assert_eq!(Frame::decode(&[4, 0, 0]), Err(FrameError::Truncated));
        assert_eq!(
            Frame::decode(&put(99, 1, &[])),
            Err(FrameError::UnknownType(99))
        );
        assert_eq!(
            Frame::decode(&put(kind::WINDOW_UPDATE, 1, &[1])),
            Err(FrameError::Truncated)
        );
        assert_eq!(
            Frame::decode(&put(kind::OPEN, 1, b"x")),
            Err(FrameError::TooLarge)
        );
        assert_eq!(
            Frame::decode(&put(kind::OPEN_FAILED, 1, &[9])),
            Err(FrameError::TooLarge)
        );
    }

    // Catches terminal output or D reaching a log line through `{:?}`.
    #[test]
    fn debug_never_prints_payload_bytes() {
        let shown = format!(
            "{:?} {:?}",
            Frame::Data {
                stream: 1,
                bytes: b"hunter2-output".to_vec()
            },
            Frame::Credential {
                bytes: b"daemon-token".to_vec()
            }
        );
        assert!(!shown.contains("hunter2") && !shown.contains("daemon-token"));
    }
}
