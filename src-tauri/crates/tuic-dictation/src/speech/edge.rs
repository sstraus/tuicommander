//! Microsoft Edge neural voices behind the speech port.
//!
//! The service is the one the Edge browser calls for "Read aloud": a WebSocket
//! that takes SSML and answers with MP3 frames. It needs no model on disk and
//! speaks Italian well, which is why it is the default engine; it needs the
//! network, which is why Pocket TTS and the external command remain.
//!
//! ## Why this is not the `msedge-tts` crate
//!
//! `msedge-tts` 0.4.0 was evaluated first (MIT OR Apache-2.0, maintained, small).
//! Its blocking client loops on a read from a socket it owns privately, so a
//! [`SpeechCancel`] raised mid-request cannot reach it, and it brings `ureq` plus
//! a second certificate-verifier setup beside the `rustls` and `tungstenite`
//! the application already ships. This file is the part of it that is needed —
//! the handshake token, two messages, a frame parser — with a read timeout on
//! the socket so the request is abandoned within [`POLL_INTERVAL`], and the
//! dial on a helper thread so a black-holed network does not hold a cancel.
//!
//! ## What is assumed about the service
//!
//! Nothing here is ours, and all of it was recorded rather than guessed
//! (`fixtures/edge_stream.json`, recorded from the live service):
//!
//! * the handshake wants `Sec-MS-GEC`, a SHA-256 of the clock rounded to five
//!   minutes and the public client token, plus the browser's `Origin`;
//! * a request is `speech.config` then `ssml`; the answer is text frames
//!   (`turn.start`, `response`, `turn.end`) around binary frames;
//! * a binary frame is a big-endian `u16` header length, the headers, then the
//!   MP3 body. Only `Path:audio` frames carry audio.
//!
//! The service is unofficial and can change. A change shows up as a typed
//! [`SpeechError::Failed`] naming what the service did, never as silence.

use std::fmt::Write;
use std::io::Cursor;
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rodio::Source;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::header;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

use super::{Speech, SpeechAudio, SpeechCancel, SpeechError, budget_seconds};

type Result<T> = std::result::Result<T, SpeechError>;

const HOST: &str = "speech.platform.bing.com";
const CLIENT_TOKEN: &str = "6A5AA1D4EAFF4E9FB37E23D68491D6F4";
const WSS_PATH: &str = "/consumer/speech/synthesize/readaloud/edge/v1";
const ORIGIN: &str = "chrome-extension://jdiccldimpdaibmpdkjnbmckianbfold";
const USER_AGENT: &str = "Mozilla/5.0 (Linux; Android 10; HD1913) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.7499.193 Mobile Safari/537.36 EdgA/143.0.3650.125";
const GEC_VERSION: &str = "1-130.0.2849.68";

/// Where the voice list lives. Fetched by the caller, which already has an
/// HTTP client; this module only parses what comes back.
pub const VOICE_LIST_URL: &str = "https://speech.platform.bing.com/consumer/speech/synthesize/readaloud/voices/list?trustedclienttoken=6A5AA1D4EAFF4E9FB37E23D68491D6F4";
pub const VOICE_LIST_USER_AGENT: &str = USER_AGENT;

/// 24 kHz mono MP3 at 48 kbit/s. Chosen here, so the audio rate is a constant
/// the budget can be computed from: 6000 bytes of MP3 are one second of speech.
const OUTPUT_FORMAT: &str = "audio-24khz-48kbitrate-mono-mp3";
const BYTES_PER_SECOND: f32 = 6000.0;

/// The service refuses SSML past about 4 KB. Longer text is sent as several
/// requests of at most this many escaped bytes and the MP3 streams are
/// joined, which decoders accept.
const MAX_TEXT_BYTES: usize = 3000;

/// How often a blocked read gives control back to look at the cancel flag and
/// the clock. This is the worst-case latency of a cancel.
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Connecting and the handshake share this ceiling; offline machines fail here.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The smallest overall timeout, whatever the text. Measured on the live
/// service: 63 s of speech arrived in about 7 s, so the floor is generous.
const TIMEOUT_FLOOR: Duration = Duration::from_secs(30);

/// One thing the service sent, as far as this module cares.
#[derive(Debug)]
enum Frame {
    Text(String),
    Binary(Vec<u8>),
    /// The service ended the conversation.
    Closed,
    /// Nothing arrived within [`POLL_INTERVAL`].
    Idle,
}

/// The connection, as the synthesis loop sees it. A trait so the loop is
/// exercised against recorded frames without a network.
trait Socket: Send {
    fn send(&mut self, text: String) -> Result<()>;
    fn recv(&mut self) -> Result<Frame>;
}

#[cfg(test)]
type Dial = Box<dyn Fn() -> Result<Box<dyn Socket>> + Send + Sync>;

/// What bounds one synthesis, shared by every request it makes.
struct Limits {
    deadline: Instant,
    /// MP3 bytes the text can justify; see [`budget_seconds`].
    max_bytes: usize,
    budget: f32,
}

/// Edge neural voices as a speech engine.
pub struct EdgeSpeech {
    dial: Arc<dyn Fn() -> Result<Box<dyn Socket>> + Send + Sync>,
    /// Set only by [`EdgeSpeech::with_timeout`]; otherwise derived from the text.
    timeout: Option<Duration>,
}

impl Default for EdgeSpeech {
    fn default() -> Self {
        Self::new()
    }
}

impl EdgeSpeech {
    pub fn new() -> Self {
        Self {
            dial: Arc::new(dial_service),
            timeout: None,
        }
    }

    #[cfg(test)]
    fn with_dial(dial: Dial) -> Self {
        Self {
            dial: Arc::from(dial),
            timeout: None,
        }
    }

    /// The tests use this so a timeout is proven in a fraction of a second.
    #[cfg(test)]
    fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    fn timeout_for(&self, budget: f32) -> Duration {
        self.timeout
            .unwrap_or_else(|| Duration::from_secs_f32(budget).max(TIMEOUT_FLOOR))
    }

    /// A connection, or the reason there is none, given up on as soon as the
    /// cancel flag rises or the deadline passes.
    ///
    /// Name resolution and `connect` block with no way to look at a flag, so
    /// they run on a helper thread and this one polls for the answer. A dial
    /// abandoned on a black-holed network ends by itself at the connect and
    /// handshake timeouts; its socket is dropped then.
    fn dial(&self, cancel: &SpeechCancel, deadline: Instant) -> Result<Box<dyn Socket>> {
        let (answer, dialled) = mpsc::channel();
        let dial = Arc::clone(&self.dial);
        std::thread::Builder::new()
            .name("edge-dial".to_string())
            .spawn(move || {
                // Nobody is listening once the request was abandoned.
                let _ = answer.send(dial());
            })
            .map_err(|error| unreachable_service(&error))?;
        loop {
            if cancel.is_cancelled() {
                return Err(SpeechError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(timed_out());
            }
            match dialled.recv_timeout(POLL_INTERVAL) {
                Ok(socket) => return socket,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(unreachable_service(&"the connection attempt failed"));
                }
            }
        }
    }

    /// One request on its own connection: the MP3 bytes for `text`, appended
    /// to `mp3`.
    fn request(
        &self,
        text: &str,
        voice: &str,
        cancel: &SpeechCancel,
        limits: &Limits,
        mp3: &mut Vec<u8>,
    ) -> Result<()> {
        let mut socket = self.dial(cancel, limits.deadline)?;
        if cancel.is_cancelled() {
            return Err(SpeechError::Cancelled);
        }
        socket.send(config_message())?;
        socket.send(ssml_message(voice, text))?;
        loop {
            if cancel.is_cancelled() {
                return Err(SpeechError::Cancelled);
            }
            if Instant::now() >= limits.deadline {
                return Err(timed_out());
            }
            match socket.recv()? {
                Frame::Idle => {}
                Frame::Closed => {
                    return Err(SpeechError::Failed(
                        "the Microsoft Edge speech service closed the connection before the end of the reply"
                            .to_string(),
                    ));
                }
                Frame::Text(message) => {
                    if message_path(&message) == Some("turn.end") {
                        return Ok(());
                    }
                }
                Frame::Binary(frame) => {
                    mp3.extend_from_slice(audio_body(&frame)?);
                    if mp3.len() > limits.max_bytes {
                        return Err(SpeechError::Runaway {
                            budget_seconds: limits.budget,
                        });
                    }
                }
            }
        }
    }
}

impl Speech for EdgeSpeech {
    fn synthesize(&self, text: &str, voice: &str, cancel: &SpeechCancel) -> Result<SpeechAudio> {
        if cancel.is_cancelled() {
            return Err(SpeechError::Cancelled);
        }
        check_voice(voice)?;
        let budget = budget_seconds(text);
        let limits = Limits {
            deadline: Instant::now() + self.timeout_for(budget),
            max_bytes: (budget * BYTES_PER_SECOND) as usize,
            budget,
        };
        let mut mp3 = Vec::new();
        for piece in split_text(text) {
            self.request(piece, voice, cancel, &limits, &mut mp3)?;
        }
        decode_mp3(mp3)
    }
}

/// A voice id reaches SSML as an attribute, so it is a name or it is refused:
/// letters, digits and hyphens, nothing that could close the attribute.
fn check_voice(voice: &str) -> Result<()> {
    let plain = !voice.is_empty()
        && voice.len() <= 64
        && voice.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    if plain {
        Ok(())
    } else {
        Err(SpeechError::UnknownVoice(voice.to_string()))
    }
}

fn timestamp() -> String {
    // The format the Edge client itself sends.
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format_timestamp(seconds)
}

/// `Thu Oct 01 2026 19:36:00 GMT+0000 (Coordinated Universal Time)`.
fn format_timestamp(unix_seconds: u64) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = unix_seconds / 86_400;
    let rest = unix_seconds % 86_400;
    // Civil-from-days, valid for every date after 1970.
    let z = days as i64 + 719_468;
    let era = z / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{} {} {day:02} {year} {:02}:{:02}:{:02} GMT+0000 (Coordinated Universal Time)",
        DAYS[(days % 7) as usize],
        MONTHS[(month - 1) as usize],
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
    )
}

fn request_id() -> String {
    format!("{:032x}", rand::random::<u128>())
}

fn config_message() -> String {
    format!(
        "X-Timestamp:{}\r\nContent-Type:application/json; charset=utf-8\r\nPath:speech.config\r\n\r\n\
         {{\"context\":{{\"synthesis\":{{\"audio\":{{\"metadataoptions\":{{\"sentenceBoundaryEnabled\":\"false\",\"wordBoundaryEnabled\":\"false\"}},\"outputFormat\":\"{OUTPUT_FORMAT}\"}}}}}}}}",
        timestamp()
    )
}

fn ssml_message(voice: &str, text: &str) -> String {
    format!(
        "X-RequestId:{}\r\nContent-Type:application/ssml+xml\r\nX-Timestamp:{}\r\nPath:ssml\r\n\r\n\
         <speak version='1.0' xmlns='http://www.w3.org/2001/10/synthesis' xml:lang='en-US'>\
         <voice name='{voice}'><prosody pitch='+0Hz' rate='+0%' volume='+0%'>{}</prosody></voice></speak>",
        request_id(),
        timestamp(),
        escape_xml(text)
    )
}

/// A transcript is arbitrary text: `<` and `&` must not become markup, and
/// characters XML 1.0 does not allow (control characters, U+FFFE, U+FFFF) are
/// dropped.
fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    text.chars().for_each(|c| push_escaped(c, &mut out));
    out
}

fn push_escaped(c: char, out: &mut String) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        '"' => out.push_str("&quot;"),
        '\'' => out.push_str("&apos;"),
        '\t' | '\n' | '\r' => out.push(c),
        c if c.is_control() || matches!(c, '\u{FFFE}' | '\u{FFFF}') => {}
        c => out.push(c),
    }
}

/// Pieces whose escaped form is at most [`MAX_TEXT_BYTES`], cut at whitespace
/// where there is one. The service limits the SSML text, and `'` becomes six
/// bytes of it, so the cut counts what is sent.
fn split_text(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut rest = text.trim();
    while !rest.is_empty() {
        let mut escaped = String::new();
        let mut cut = rest.len();
        for (at, c) in rest.char_indices() {
            push_escaped(c, &mut escaped);
            if escaped.len() > MAX_TEXT_BYTES {
                cut = rest[..at]
                    .rfind(char::is_whitespace)
                    .filter(|&space| space > 0)
                    .unwrap_or(at);
                break;
            }
        }
        pieces.push(rest[..cut].trim_end());
        rest = rest[cut..].trim_start();
    }
    pieces
}

/// The `Path:` header of a text frame.
fn message_path(message: &str) -> Option<&str> {
    let headers = message.split("\r\n\r\n").next()?;
    headers
        .lines()
        .find_map(|line| line.strip_prefix("Path:"))
        .map(str::trim)
}

/// The MP3 bytes of a binary frame; empty for a frame that is not audio.
fn audio_body(frame: &[u8]) -> Result<&[u8]> {
    let malformed = || {
        SpeechError::Failed("the Microsoft Edge speech service sent a malformed audio frame".into())
    };
    let length = frame.get(..2).ok_or_else(malformed)?;
    let header_end = 2 + usize::from(u16::from_be_bytes([length[0], length[1]]));
    let headers = frame.get(2..header_end).ok_or_else(malformed)?;
    if String::from_utf8_lossy(headers).contains("Path:audio") {
        Ok(&frame[header_end..])
    } else {
        Ok(&[])
    }
}

/// MP3 to mono PCM at the stream's own rate.
fn decode_mp3(mp3: Vec<u8>) -> Result<SpeechAudio> {
    let empty =
        || SpeechError::Failed("the Microsoft Edge speech service returned no audio".into());
    if mp3.is_empty() {
        return Err(empty());
    }
    let decoder = rodio::Decoder::new_mp3(Cursor::new(mp3)).map_err(|error| {
        SpeechError::Failed(format!("could not decode the service audio: {error}"))
    })?;
    let channels = usize::from(decoder.channels().get());
    let sample_rate = decoder.sample_rate().get();
    let mut samples = Vec::new();
    let mut frame_sum = 0.0f32;
    let mut in_frame = 0usize;
    for sample in decoder {
        frame_sum += sample;
        in_frame += 1;
        if in_frame == channels {
            samples.push(frame_sum / channels as f32);
            frame_sum = 0.0;
            in_frame = 0;
        }
    }
    if samples.is_empty() {
        return Err(empty());
    }
    Ok(SpeechAudio {
        samples,
        sample_rate,
    })
}

/// `Sec-MS-GEC`: SHA-256 of the Windows file-time clock rounded down to five
/// minutes, followed by the client token, upper-case hex.
fn sec_ms_gec(now: SystemTime) -> String {
    const WINDOWS_EPOCH_OFFSET_SECONDS: u64 = 11_644_473_600;
    const FIVE_MINUTES_IN_TICKS: u128 = 3_000_000_000;
    let since_unix = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let ticks = (since_unix + Duration::from_secs(WINDOWS_EPOCH_OFFSET_SECONDS)).as_nanos() / 100;
    let ticks = ticks - ticks % FIVE_MINUTES_IN_TICKS;
    let digest = Sha256::digest(format!("{ticks}{CLIENT_TOKEN}"));
    let mut token = String::with_capacity(64);
    for byte in digest {
        write!(token, "{byte:02X}").expect("writing to a String cannot fail");
    }
    token
}

// ---------------------------------------------------------------------------
// The real connection
// ---------------------------------------------------------------------------

struct WsSocket(WebSocket<MaybeTlsStream<TcpStream>>);

impl Socket for WsSocket {
    fn send(&mut self, text: String) -> Result<()> {
        self.0
            .send(Message::Text(text.into()))
            .map_err(|error| connection_failed(&error))
    }

    fn recv(&mut self) -> Result<Frame> {
        use tungstenite::Error;
        match self.0.read() {
            Ok(Message::Text(text)) => Ok(Frame::Text(text.as_str().to_string())),
            Ok(Message::Binary(bytes)) => Ok(Frame::Binary(bytes.to_vec())),
            Ok(Message::Close(_)) => Ok(Frame::Closed),
            Ok(_) => Ok(Frame::Idle),
            Err(Error::Io(error))
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                Ok(Frame::Idle)
            }
            Err(Error::ConnectionClosed | Error::AlreadyClosed) => Ok(Frame::Closed),
            Err(error) => Err(connection_failed(&error)),
        }
    }
}

fn rejected(status: u16) -> SpeechError {
    if matches!(status, 401 | 403) {
        SpeechError::Rejected { status }
    } else {
        SpeechError::Failed(format!(
            "the Microsoft Edge speech service rejected the request (HTTP {status})"
        ))
    }
}

fn connection_failed(error: &tungstenite::Error) -> SpeechError {
    SpeechError::Failed(format!(
        "the connection to the Microsoft Edge speech service failed: {error}"
    ))
}

fn timed_out() -> SpeechError {
    SpeechError::Failed("the Microsoft Edge speech service did not finish in time".to_string())
}

fn unreachable_service(reason: &dyn std::fmt::Display) -> SpeechError {
    SpeechError::Failed(format!(
        "cannot reach the Microsoft Edge speech service ({reason}); it needs an internet connection"
    ))
}

fn handshake_request() -> Result<tungstenite::handshake::client::Request> {
    let url = format!(
        "wss://{HOST}{WSS_PATH}?TrustedClientToken={CLIENT_TOKEN}&ConnectionId={}&Sec-MS-GEC={}&Sec-MS-GEC-Version={GEC_VERSION}",
        request_id(),
        sec_ms_gec(SystemTime::now()),
    );
    let mut request = url
        .into_client_request()
        .map_err(|error| SpeechError::Failed(format!("bad speech service address: {error}")))?;
    let headers = request.headers_mut();
    headers.insert(header::PRAGMA, header::HeaderValue::from_static("no-cache"));
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache"),
    );
    headers.insert(
        header::USER_AGENT,
        header::HeaderValue::from_static(USER_AGENT),
    );
    headers.insert(header::ORIGIN, header::HeaderValue::from_static(ORIGIN));
    Ok(request)
}

// DEFERRED (2026-10-01) — the system proxy. The voice list goes through reqwest
// and honours it; this raw `TcpStream` does not, so behind a corporate proxy the
// picker loads and speech fails with "cannot reach". Honouring it means finding
// the proxy (environment, macOS and Windows settings, PAC) and an HTTP CONNECT
// tunnel under the TLS handshake; not a small change, and no user has asked yet.
fn dial_service() -> Result<Box<dyn Socket>> {
    let request = handshake_request()?;
    let addresses = (HOST, 443)
        .to_socket_addrs()
        .map_err(|error| unreachable_service(&error))?;
    let mut last_error = None;
    let mut tcp = None;
    for address in addresses {
        match TcpStream::connect_timeout(&address, CONNECT_TIMEOUT) {
            Ok(stream) => {
                tcp = Some(stream);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let tcp = tcp.ok_or_else(|| match last_error {
        Some(error) => unreachable_service(&error),
        None => unreachable_service(&"no address"),
    })?;
    // The handshake may block for as long as the connect did; after it, reads
    // wake every POLL_INTERVAL. The clone shares the socket, so the option set
    // on it applies to the stream the WebSocket owns.
    let control = tcp
        .try_clone()
        .map_err(|error| unreachable_service(&error))?;
    tcp.set_nodelay(true).ok();
    tcp.set_read_timeout(Some(CONNECT_TIMEOUT))
        .and_then(|()| tcp.set_write_timeout(Some(CONNECT_TIMEOUT)))
        .map_err(|error| unreachable_service(&error))?;
    let (socket, _) = tungstenite::client_tls(request, tcp).map_err(|error| match error {
        tungstenite::HandshakeError::Failure(tungstenite::Error::Http(response)) => {
            rejected(response.status().as_u16())
        }
        tungstenite::HandshakeError::Failure(error) => unreachable_service(&error),
        tungstenite::HandshakeError::Interrupted(_) => {
            unreachable_service(&"the handshake timed out")
        }
    })?;
    control
        .set_read_timeout(Some(POLL_INTERVAL))
        .map_err(|error| unreachable_service(&error))?;
    Ok(Box::new(WsSocket(socket)))
}

// ---------------------------------------------------------------------------
// Voices
// ---------------------------------------------------------------------------

/// One entry of the service's voice list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EdgeVoice {
    /// What a configuration stores and the SSML names: `it-IT-IsabellaNeural`.
    pub id: String,
    pub locale: String,
    pub gender: String,
    /// The label for a picker: `Microsoft Isabella Online (Natural) - Italian (Italy)`.
    pub label: String,
}

/// Parse the service's voice list.
pub fn parse_voices(json: &str) -> std::result::Result<Vec<EdgeVoice>, String> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Entry {
        short_name: String,
        locale: String,
        #[serde(default)]
        gender: String,
        #[serde(default)]
        friendly_name: String,
    }
    let entries: Vec<Entry> =
        serde_json::from_str(json).map_err(|e| format!("unreadable voice list: {e}"))?;
    Ok(entries
        .into_iter()
        .map(|entry| EdgeVoice {
            label: if entry.friendly_name.is_empty() {
                entry.short_name.clone()
            } else {
                entry.friendly_name
            },
            id: entry.short_name,
            locale: entry.locale,
            gender: entry.gender,
        })
        .collect())
}

/// The voices that speak a dictation language (`it`, `en`, …). The match is on
/// the whole language subtag, so `i` or `it-` do not select Italian and `iu`
/// does not.
pub fn voices_for_language(voices: &[EdgeVoice], language: &str) -> Vec<EdgeVoice> {
    if language.is_empty() {
        return Vec::new();
    }
    voices
        .iter()
        .filter(|voice| voice.locale.split('-').next() == Some(language))
        .cloned()
        .collect()
}

/// The voice a language speaks with when none is chosen. Verified against the
/// live list on 2026-10-01 for every language the settings offer
/// (`WHISPER_LANGUAGES`); a language not listed has no default and the user
/// is asked to choose from the service list.
const DEFAULT_VOICES: &[(&str, &str)] = &[
    ("it", "it-IT-IsabellaNeural"),
    ("en", "en-US-AriaNeural"),
    ("fr", "fr-FR-DeniseNeural"),
    ("de", "de-DE-KatjaNeural"),
    ("es", "es-ES-ElviraNeural"),
    ("pt", "pt-BR-FranciscaNeural"),
    ("ja", "ja-JP-NanamiNeural"),
    ("zh", "zh-CN-XiaoxiaoNeural"),
    ("ko", "ko-KR-SunHiNeural"),
    ("nl", "nl-NL-ColetteNeural"),
    ("ru", "ru-RU-SvetlanaNeural"),
];

/// Which voice speaks `language`.
///
/// The configured voice is used when it speaks that language (or is
/// multilingual). Otherwise the language default: the setting holds one voice,
/// the conversation language can change under Auto, and an Italian voice
/// reading English is worse than the default for English.
pub fn choose_voice(language: &str, configured: &str) -> std::result::Result<String, String> {
    let speaks_language =
        configured.split('-').next() == Some(language) || configured.contains("Multilingual");
    if !configured.is_empty() && speaks_language {
        return Ok(configured.to_string());
    }
    DEFAULT_VOICES
        .iter()
        .find(|(code, _)| *code == language)
        .map(|(_, voice)| (*voice).to_string())
        .ok_or_else(|| {
            format!(
                "No default Microsoft Edge voice for language \"{language}\"; choose one in Settings → Voice"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    const STREAM: &str = include_str!("fixtures/edge_stream.json");
    const VOICES: &str = include_str!("fixtures/edge_voices.json");

    /// The recorded conversation, as frames in the order the service sent them.
    fn recorded_frames() -> Vec<Frame> {
        #[derive(serde::Deserialize)]
        struct Recording {
            frames: Vec<serde_json::Value>,
        }
        let recording: Recording = serde_json::from_str(STREAM).expect("fixture parses");
        recording
            .frames
            .iter()
            .map(|frame| match frame["kind"].as_str() {
                Some("text") => Frame::Text(frame["data"].as_str().unwrap().to_string()),
                Some("binary") => Frame::Binary(base64_decode(frame["b64"].as_str().unwrap())),
                other => panic!("unknown frame kind {other:?}"),
            })
            .collect()
    }

    fn base64_decode(text: &str) -> Vec<u8> {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let (mut buffer, mut bits) = (0u32, 0u32);
        for byte in text.bytes().filter(|b| *b != b'=') {
            let value = ALPHABET.iter().position(|a| *a == byte).unwrap() as u32;
            buffer = buffer << 6 | value;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buffer >> bits) as u8);
                buffer &= (1 << bits) - 1;
            }
        }
        out
    }

    /// A scripted connection: plays `frames`, then `then`, and records what was sent.
    struct Script {
        frames: VecDeque<Frame>,
        then: fn() -> Frame,
        sent: Arc<Mutex<Vec<String>>>,
    }

    impl Socket for Script {
        fn send(&mut self, text: String) -> Result<()> {
            self.sent.lock().unwrap().push(text);
            Ok(())
        }
        fn recv(&mut self) -> Result<Frame> {
            Ok(self.frames.pop_front().unwrap_or_else(|| {
                std::thread::sleep(Duration::from_millis(5));
                (self.then)()
            }))
        }
    }

    fn engine_playing(
        frames: Vec<Frame>,
        then: fn() -> Frame,
    ) -> (EdgeSpeech, Arc<Mutex<Vec<String>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::clone(&sent);
        let frames = Arc::new(Mutex::new(Some(frames)));
        let engine = EdgeSpeech::with_dial(Box::new(move || {
            let frames = frames.lock().unwrap().take().unwrap_or_default();
            Ok(Box::new(Script {
                frames: frames.into(),
                then,
                sent: Arc::clone(&shared),
            }))
        }));
        (engine, sent)
    }

    const SENTENCE: &str = "Ciao Boss, il pannello è su main.";

    #[test]
    fn only_auth_http_rejections_trigger_cooldown_without_clock_advice() {
        // catches: 403 blames the clock, or a transient server failure disables speech.
        for status in [401, 403] {
            let error = rejected(status);
            assert!(matches!(error, SpeechError::Rejected { .. }));
            assert!(error.to_string().contains(&format!("HTTP {status}")));
            assert!(!error.to_string().contains("clock"));
        }
        assert!(matches!(rejected(500), SpeechError::Failed(_)));
    }

    #[test]
    fn the_recorded_stream_decodes_to_audible_mono_speech_at_the_streams_rate() {
        // Catches: feeding the frame headers to the MP3 decoder (wrong body
        // offset), which yields a decode error or noise instead of speech.
        let (engine, _) = engine_playing(recorded_frames(), || Frame::Idle);
        let audio = engine
            .synthesize(SENTENCE, "it-IT-IsabellaNeural", &SpeechCancel::new())
            .expect("the recorded stream decodes");

        assert_eq!(audio.sample_rate, 24_000);
        let seconds = audio.duration_seconds();
        assert!((1.5..6.0).contains(&seconds), "{seconds}s for one sentence");
        let peak = audio.samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        assert!((0.05..=1.0).contains(&peak), "peak {peak}");
        assert!(seconds < budget_seconds(SENTENCE));
    }

    #[test]
    fn cancelling_while_the_service_is_silent_stops_the_request() {
        // Catches: a read loop that only checks the flag between frames, so an
        // abandoned reply on a stalled connection hangs until the deadline.
        let (engine, _) = engine_playing(vec![recorded_frames().remove(0)], || Frame::Idle);
        let cancel = SpeechCancel::new();
        let from_another_thread = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            from_another_thread.cancel();
        });
        let started = Instant::now();
        let result = engine.synthesize(SENTENCE, "it-IT-IsabellaNeural", &cancel);

        assert_eq!(result, Err(SpeechError::Cancelled));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_request_already_cancelled_never_dials() {
        // Catches: opening a TLS connection for a reply nobody wants.
        let dialled = Arc::new(Mutex::new(false));
        let flag = Arc::clone(&dialled);
        let engine = EdgeSpeech::with_dial(Box::new(move || {
            *flag.lock().unwrap() = true;
            Err(SpeechError::Failed("must not dial".into()))
        }));
        let cancel = SpeechCancel::new();
        cancel.cancel();

        assert_eq!(
            engine.synthesize(SENTENCE, "it-IT-IsabellaNeural", &cancel),
            Err(SpeechError::Cancelled)
        );
        assert!(!*dialled.lock().unwrap());
    }

    #[test]
    fn a_service_that_never_finishes_fails_instead_of_hanging() {
        // Catches: no deadline on the read loop — the user hears nothing and
        // the reply queue stays "rendering" forever.
        let (engine, _) = engine_playing(Vec::new(), || Frame::Idle);
        let engine = engine.with_timeout(Duration::from_millis(200));
        let result = engine.synthesize(SENTENCE, "it-IT-IsabellaNeural", &SpeechCancel::new());

        match result {
            Err(SpeechError::Failed(reason)) => assert!(reason.contains("did not finish")),
            other => panic!("expected a timeout, got {other:?}"),
        }
    }

    #[test]
    fn audio_that_outgrows_the_text_is_stopped_as_a_runaway() {
        // Catches: an unbounded download when the service loops on one reply;
        // "Sì." may not produce more than the budget's worth of audio.
        let frame = recorded_frames()
            .into_iter()
            .find(|f| matches!(f, Frame::Binary(b) if b.len() > 800))
            .expect("fixture has audio frames");
        let Frame::Binary(bytes) = frame else {
            unreachable!()
        };
        let endless: fn() -> Frame = || unreachable!();
        let frames = (0..200).map(|_| Frame::Binary(bytes.clone())).collect();
        let (engine, _) = engine_playing(frames, endless);
        let result = engine.synthesize("Sì.", "it-IT-IsabellaNeural", &SpeechCancel::new());

        assert!(
            matches!(result, Err(SpeechError::Runaway { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn a_connection_closed_before_the_end_is_an_error_not_truncated_speech() {
        // Catches: speaking half a sentence when the service drops the socket.
        let mut frames = recorded_frames();
        frames.truncate(5);
        let (engine, _) = engine_playing(frames, || Frame::Closed);
        let result = engine.synthesize(SENTENCE, "it-IT-IsabellaNeural", &SpeechCancel::new());

        match result {
            Err(SpeechError::Failed(reason)) => assert!(reason.contains("closed the connection")),
            other => panic!("expected a closed-connection error, got {other:?}"),
        }
    }

    #[test]
    fn a_dial_failure_reaches_the_caller_unchanged() {
        // Catches: swallowing the offline message into silence.
        let engine = EdgeSpeech::with_dial(Box::new(|| {
            Err(SpeechError::Failed("cannot reach it".into()))
        }));
        assert_eq!(
            engine.synthesize(SENTENCE, "it-IT-IsabellaNeural", &SpeechCancel::new()),
            Err(SpeechError::Failed("cannot reach it".into()))
        );
    }

    #[test]
    fn a_voice_that_could_close_the_ssml_attribute_is_refused_before_dialling() {
        // Catches: SSML injection through the voice setting.
        let engine = EdgeSpeech::with_dial(Box::new(|| unreachable!("must not dial")));
        for voice in [
            "",
            "it-IT x",
            "a'><break/>",
            "it-IT-IsabellaNeural\r\nPath:x",
        ] {
            assert_eq!(
                engine.synthesize(SENTENCE, voice, &SpeechCancel::new()),
                Err(SpeechError::UnknownVoice(voice.to_string()))
            );
        }
    }

    #[test]
    fn the_spoken_text_is_escaped_so_a_transcript_cannot_inject_markup() {
        // Catches: `<` or `&` in a reply breaking the SSML (the service
        // answers with an error) or adding elements of the sender's choosing.
        let (engine, sent) = engine_playing(recorded_frames(), || Frame::Idle);
        engine
            .synthesize(
                "a < b & <break/> \u{7}c",
                "it-IT-IsabellaNeural",
                &SpeechCancel::new(),
            )
            .expect("synthesizes");
        let sent = sent.lock().unwrap();
        let ssml = sent.iter().find(|m| m.contains("Path:ssml")).unwrap();

        assert!(ssml.contains(">a &lt; b &amp; &lt;break/&gt; c<"), "{ssml}");
        assert!(sent.iter().any(|m| m.contains("Path:speech.config")));
        assert!(sent[0].contains(OUTPUT_FORMAT));
    }

    #[test]
    fn long_text_is_split_on_char_boundaries_below_the_service_limit() {
        // Catches: a byte-offset cut inside "è" (panic) and a request over the
        // service's size limit (rejected reply). The limit is on what is sent,
        // so it is measured on the escaped text against the service's 4 KB.
        let text = "perché è così. ".repeat(500);
        let pieces = split_text(&text);

        assert!(pieces.len() > 1);
        assert!(pieces.iter().all(|p| escape_xml(p).len() <= 4096));
        let rejoined = pieces.join(" ");
        assert_eq!(
            rejoined.split_whitespace().count(),
            text.split_whitespace().count()
        );
        assert!(split_text("   ").is_empty());
    }

    #[test]
    fn a_frame_too_short_for_its_own_header_is_an_error_not_a_panic() {
        // Catches: slicing past the end on a truncated frame from the service.
        assert!(audio_body(&[0x00]).is_err());
        assert!(audio_body(&[0x00, 0x10, b'P']).is_err());
        assert_eq!(
            audio_body(&[0x00, 0x04, b'a', b'b', b'c', b'd', 9]).unwrap(),
            &[] as &[u8]
        );
    }

    #[test]
    fn only_path_audio_frames_contribute_bytes() {
        // Catches: a metadata or unknown binary frame being decoded as MP3.
        let mut frame = vec![0x00, 10];
        frame.extend_from_slice(b"Path:audio");
        frame.extend_from_slice(&[1, 2, 3]);
        assert_eq!(audio_body(&frame).unwrap(), &[1, 2, 3]);
    }

    #[test]
    fn the_end_of_the_turn_is_recognised_by_its_path_header() {
        let frames = recorded_frames();
        let paths: Vec<_> = frames
            .iter()
            .filter_map(|f| match f {
                Frame::Text(t) => message_path(t),
                _ => None,
            })
            .collect();
        assert_eq!(paths, ["turn.start", "response", "turn.end"]);
    }

    #[test]
    fn the_handshake_token_matches_the_reference_implementation() {
        // Reference value computed with the Python `edge-tts` algorithm for
        // 2026-10-01T19:36:20Z. A wrong rounding or epoch makes the service
        // answer 403 for everybody.
        let at = UNIX_EPOCH + Duration::from_secs(1_790_883_380);
        assert_eq!(
            sec_ms_gec(at),
            "A95E984BF41D3C7B98C20460112A5DD785722A04FC99D8110F2ED9703A010F73"
        );
    }

    #[test]
    fn timestamps_use_the_browser_format() {
        assert_eq!(
            format_timestamp(1_790_883_380),
            "Thu Oct 01 2026 19:36:20 GMT+0000 (Coordinated Universal Time)"
        );
        assert_eq!(
            format_timestamp(0),
            "Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)"
        );
        assert_eq!(
            format_timestamp(1_709_164_800),
            "Thu Feb 29 2024 00:00:00 GMT+0000 (Coordinated Universal Time)"
        );
    }

    #[test]
    fn the_recorded_voice_list_filters_by_whole_language_subtag() {
        // Catches: prefix matching, where "i" or "it-" selects Italian and "en"
        // swallows other languages whose code starts the same way.
        let voices = parse_voices(VOICES).expect("recorded list parses");
        let italian: Vec<_> = voices_for_language(&voices, "it")
            .into_iter()
            .map(|v| v.id)
            .collect();

        assert_eq!(
            italian,
            [
                "it-IT-GiuseppeMultilingualNeural",
                "it-IT-DiegoNeural",
                "it-IT-ElsaNeural",
                "it-IT-IsabellaNeural"
            ]
        );
        assert!(voices_for_language(&voices, "i").is_empty());
        assert!(voices_for_language(&voices, "").is_empty());
        assert!(voices_for_language(&voices, "auto").is_empty());
        assert!(
            voices_for_language(&voices, "en")
                .iter()
                .all(|v| v.locale == "en-US")
        );
    }

    #[test]
    fn an_unreadable_voice_list_is_reported() {
        assert!(parse_voices("<html>").is_err());
    }

    #[test]
    fn every_default_voice_is_in_the_recorded_list() {
        // Catches: a default that the service does not offer (typo, rename). The
        // fixture holds one entry for every default, so none is skipped.
        let voices = parse_voices(VOICES).unwrap();
        for (language, voice) in DEFAULT_VOICES {
            let offered = voices_for_language(&voices, language);
            assert!(offered.iter().any(|v| v.id == *voice), "{voice}");
        }
    }

    #[test]
    fn a_configured_voice_for_another_language_falls_back_to_the_language_default() {
        // Catches: an Italian voice reading an English reply after the
        // conversation language changed under Auto.
        assert_eq!(
            choose_voice("en", "it-IT-ElsaNeural").unwrap(),
            "en-US-AriaNeural"
        );
        assert_eq!(
            choose_voice("it", "it-IT-ElsaNeural").unwrap(),
            "it-IT-ElsaNeural"
        );
        assert_eq!(choose_voice("it", "").unwrap(), "it-IT-IsabellaNeural");
        assert_eq!(
            choose_voice("en", "it-IT-GiuseppeMultilingualNeural").unwrap(),
            "it-IT-GiuseppeMultilingualNeural"
        );
    }

    #[test]
    fn a_language_without_a_default_asks_the_user_to_choose() {
        // Catches: silently speaking in a voice of another language.
        let error = choose_voice("sv", "").unwrap_err();
        assert!(error.contains("choose one"), "{error}");
        assert_eq!(
            choose_voice("sv", "sv-SE-SofieNeural").unwrap(),
            "sv-SE-SofieNeural"
        );
    }

    /// Live check against the real service. Not part of any suite:
    /// `cargo nextest run -E 'test(live_edge_service)' --run-ignored only`.
    #[test]
    #[ignore = "needs the internet"]
    fn live_edge_service_speaks_one_italian_sentence() {
        // The application installs this at startup (`lib.rs`); a test binary has no such step.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let audio = EdgeSpeech::new()
            .synthesize(SENTENCE, "it-IT-IsabellaNeural", &SpeechCancel::new())
            .expect("the service answers");
        assert_eq!(audio.sample_rate, 24_000);
        assert!(
            audio.duration_seconds() > 1.0,
            "{}s",
            audio.duration_seconds()
        );
    }
}

/// Adversarial cases from the critic of 1357-7d37 (round 1).
#[cfg(test)]
mod critic_round1 {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    pub(super) const VOICE: &str = "it-IT-IsabellaNeural";

    /// What a fake connection does on each `recv`; once the script is empty
    /// it behaves like a mute service (a read timeout every 10 ms).
    struct Scripted {
        frames: VecDeque<Result<Frame>>,
        sent: Arc<Mutex<Vec<String>>>,
    }

    impl Socket for Scripted {
        fn send(&mut self, text: String) -> Result<()> {
            self.sent.lock().unwrap().push(text);
            Ok(())
        }
        fn recv(&mut self) -> Result<Frame> {
            match self.frames.pop_front() {
                Some(frame) => frame,
                None => {
                    std::thread::sleep(Duration::from_millis(10));
                    Ok(Frame::Idle)
                }
            }
        }
    }

    /// A dial whose n-th connection plays `script(n)`; counts the dials.
    pub(super) fn dial_with(
        script: impl Fn(usize) -> Vec<Result<Frame>> + Send + Sync + 'static,
    ) -> (EdgeSpeech, Arc<AtomicUsize>, Arc<Mutex<Vec<String>>>) {
        let dials = Arc::new(AtomicUsize::new(0));
        let sent = Arc::new(Mutex::new(Vec::new()));
        let (d, s) = (Arc::clone(&dials), Arc::clone(&sent));
        let speech = EdgeSpeech::with_dial(Box::new(move || {
            let n = d.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Scripted {
                frames: script(n).into(),
                sent: Arc::clone(&s),
            }))
        }));
        (speech, dials, sent)
    }

    pub(super) fn audio_frame(body: &[u8]) -> Result<Frame> {
        let headers =
            b"X-RequestId:abc\r\nContent-Type:audio/mpeg\r\nX-StreamId:1\r\nPath:audio\r\n";
        let mut frame = (headers.len() as u16).to_be_bytes().to_vec();
        frame.extend_from_slice(headers);
        frame.extend_from_slice(body);
        Ok(Frame::Binary(frame))
    }

    pub(super) fn turn_end() -> Result<Frame> {
        Ok(Frame::Text(
            "X-RequestId:abc\r\nPath:turn.end\r\n\r\n{}".to_string(),
        ))
    }

    fn unbase64(text: &str) -> Vec<u8> {
        let mut out = Vec::new();
        let (mut acc, mut bits) = (0u32, 0);
        for c in text.bytes() {
            let v = match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => continue,
            };
            acc = (acc << 6) | u32::from(v);
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((acc >> bits) as u8);
            }
        }
        out
    }

    /// The MP3 of the recorded stream, parsed here without the code under test.
    pub(super) fn recorded_mp3() -> Vec<u8> {
        let doc: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/edge_stream.json")).unwrap();
        let mut mp3 = Vec::new();
        for frame in doc["frames"].as_array().unwrap() {
            if frame["kind"] != "binary" {
                continue;
            }
            let bytes = unbase64(frame["b64"].as_str().unwrap());
            let header = usize::from(u16::from_be_bytes([bytes[0], bytes[1]]));
            if String::from_utf8_lossy(&bytes[2..2 + header]).contains("Path:audio") {
                mp3.extend_from_slice(&bytes[2 + header..]);
            }
        }
        assert!(mp3.len() > 1000, "fixture lost its audio");
        mp3
    }

    pub(super) fn synth(speech: &EdgeSpeech, text: &str) -> Result<SpeechAudio> {
        speech.synthesize(text, VOICE, &SpeechCancel::new())
    }

    #[test]
    fn a_cut_piece_still_fits_the_service_limit_once_escaped() {
        // Catches: split_text counting raw bytes while the service limits the
        // escaped SSML text (~4 KB): apostrophes become 6 bytes, `&` 5.
        let text = "Dell'uomo & l'altro ".repeat(400);
        for piece in split_text(&text) {
            let escaped = escape_xml(piece).len();
            assert!(escaped <= 4096, "a piece of {escaped} escaped bytes");
        }
    }

    #[test]
    fn xml_noncharacters_never_reach_the_ssml() {
        // Catches: is_control() as the XML filter; U+FFFE/U+FFFF and lone
        // control-range code points are not legal XML 1.0 characters.
        let escaped = escape_xml("a\u{FFFE}b\u{FFFF}c\u{0}d\u{B}e");
        assert_eq!(escaped, "abcde");
    }

    #[test]
    fn markup_in_the_text_cannot_open_a_second_voice() {
        // Catches: unescaped text closing prosody/voice and selecting another voice.
        let ssml = ssml_message(
            VOICE,
            "</prosody></voice><voice name='en-US-AriaNeural'>hi & bye",
        );
        assert_eq!(ssml.matches("<voice ").count(), 1, "{ssml}");
        assert!(ssml.contains("&lt;/prosody&gt;"), "{ssml}");
        assert!(ssml.contains("hi &amp; bye"), "{ssml}");
    }

    #[test]
    fn a_voice_that_could_close_the_attribute_is_refused_before_dialling() {
        // Catches: a configured voice id reaching `<voice name='…'>` verbatim.
        let (speech, dials, _) = dial_with(|_| vec![audio_frame(b"x"), turn_end()]);
        for voice in [
            "it-IT-A' x='y",
            "it-IT-A\"",
            "",
            "it IT",
            "it-IT-A\n",
            "it-IT-Ünï",
            &"a".repeat(65),
        ] {
            let result = speech.synthesize("ciao", voice, &SpeechCancel::new());
            assert!(
                matches!(result, Err(SpeechError::UnknownVoice(_))),
                "{voice:?} -> {result:?}"
            );
        }
        assert_eq!(dials.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_mute_service_is_abandoned_within_a_poll_of_the_cancel() {
        // Catches: the loop blocking on the socket without looking at the flag.
        let (speech, _, _) = dial_with(|_| vec![]);
        let cancel = SpeechCancel::new();
        let remote = cancel.clone();
        let start = Instant::now();
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            remote.cancel();
        });
        let result = speech.synthesize("ciao a tutti", VOICE, &cancel);
        trigger.join().unwrap();
        assert!(matches!(result, Err(SpeechError::Cancelled)), "{result:?}");
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn a_service_that_goes_quiet_after_the_request_fails_at_the_deadline() {
        // Catches: a half-open connection hanging forever instead of a typed error.
        let (speech, _, _) = dial_with(|_| vec![audio_frame(b"\xff\xfb")]);
        let speech = speech.with_timeout(Duration::from_millis(200));
        let start = Instant::now();
        let result = synth(&speech, "ciao");
        assert!(
            matches!(&result, Err(SpeechError::Failed(m)) if m.contains("in time")),
            "{result:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_dial_that_blocks_like_a_black_holed_address_is_abandoned_at_the_cancel() {
        // Catches: name resolution and connect (blocking, up to 10 s per address)
        // holding a hushed reply, and with it the single speaker worker.
        let speech = EdgeSpeech::with_dial(Box::new(|| {
            std::thread::sleep(Duration::from_secs(5));
            Err(SpeechError::Failed("cannot reach it".into()))
        }));
        let cancel = SpeechCancel::new();
        let remote = cancel.clone();
        let trigger = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            remote.cancel();
        });
        let start = Instant::now();
        let result = speech.synthesize("ciao a tutti", VOICE, &cancel);
        trigger.join().unwrap();
        assert!(matches!(result, Err(SpeechError::Cancelled)), "{result:?}");
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn a_dial_that_never_answers_ends_at_the_deadline() {
        // Catches: the dial sitting outside the time limit, so a black-holed
        // network hangs for connect timeouts that no deadline covers.
        let speech = EdgeSpeech::with_dial(Box::new(|| {
            std::thread::sleep(Duration::from_secs(5));
            Err(SpeechError::Failed("cannot reach it".into()))
        }))
        .with_timeout(Duration::from_millis(200));
        let start = Instant::now();
        let result = synth(&speech, "ciao");
        assert!(
            matches!(&result, Err(SpeechError::Failed(m)) if m.contains("in time")),
            "{result:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_cancel_raised_while_dialling_sends_nothing() {
        // Catches: the request being written to a connection nobody wants.
        let sent = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&sent);
        let cancel = SpeechCancel::new();
        let remote = cancel.clone();
        let speech = EdgeSpeech::with_dial(Box::new(move || {
            remote.cancel();
            std::thread::sleep(Duration::from_millis(50));
            Ok(Box::new(Scripted {
                frames: VecDeque::new(),
                sent: Arc::clone(&s),
            }))
        }));
        let result = speech.synthesize("ciao", VOICE, &cancel);
        assert!(matches!(result, Err(SpeechError::Cancelled)), "{result:?}");
        assert!(sent.lock().unwrap().is_empty());
    }

    #[test]
    fn an_offline_dial_error_reaches_the_caller_unchanged() {
        // Catches: the network error being swallowed into silence or a generic message.
        let speech = EdgeSpeech::with_dial(Box::new(|| {
            Err(SpeechError::Failed(
                "cannot reach it needs an internet connection".into(),
            ))
        }));
        let result = synth(&speech, "ciao");
        assert!(
            matches!(&result, Err(SpeechError::Failed(m)) if m.contains("internet")),
            "{result:?}"
        );
    }

    #[test]
    fn audio_past_what_the_text_justifies_is_a_runaway_not_a_hang() {
        // Catches: the byte budget not being enforced across frames.
        let over = (budget_seconds("hi") * BYTES_PER_SECOND) as usize + 1;
        let (speech, _, _) = dial_with(move |_| {
            (0..4)
                .map(|_| audio_frame(&vec![0u8; over / 3 + 1]))
                .collect()
        });
        let result = synth(&speech, "hi");
        assert!(
            matches!(result, Err(SpeechError::Runaway { .. })),
            "{result:?}"
        );
    }

    #[test]
    fn malformed_binary_frames_are_a_typed_failure() {
        // Catches: slicing past the end of a short or lying frame (panic).
        for frame in [vec![0u8], vec![0, 50, b'x'], vec![0xFF, 0xFF], vec![]] {
            let bytes = frame.clone();
            let (speech, _, _) = dial_with(move |_| vec![Ok(Frame::Binary(bytes.clone()))]);
            let result = synth(&speech, "ciao");
            assert!(
                matches!(&result, Err(SpeechError::Failed(m)) if m.contains("malformed")),
                "{frame:?} -> {result:?}"
            );
        }
    }

    #[test]
    fn the_service_closing_mid_reply_is_a_typed_failure() {
        // Catches: Closed read as a clean end, returning half a sentence.
        let (speech, _, _) =
            dial_with(|_| vec![audio_frame(&recorded_mp3()[..400]), Ok(Frame::Closed)]);
        let result = synth(&speech, "ciao a tutti quanti");
        assert!(
            matches!(&result, Err(SpeechError::Failed(m)) if m.contains("closed")),
            "{result:?}"
        );
    }

    #[test]
    fn a_reply_with_no_audio_is_an_error_not_silence() {
        // Catches: turn.end with no audio frames returning Ok(empty) and playing nothing.
        let (speech, _, _) = dial_with(|_| vec![turn_end()]);
        assert!(matches!(
            synth(&speech, "ciao"),
            Err(SpeechError::Failed(_))
        ));
    }

    #[test]
    fn text_with_nothing_to_say_does_not_dial() {
        // Catches: a network round trip (and a hang offline) for an empty reply.
        let (speech, dials, _) = dial_with(|_| vec![turn_end()]);
        assert!(synth(&speech, "   \n ").is_err());
        assert_eq!(dials.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_stream_cut_mid_frame_never_yields_empty_audio() {
        // Catches: a truncated MP3 decoding to Ok with zero samples (silence).
        let mp3 = recorded_mp3();
        let full = {
            let m = mp3.clone();
            let (speech, _, _) = dial_with(move |_| vec![audio_frame(&m), turn_end()]);
            synth(&speech, "Ciao Boss, il pannello è su main.").expect("the recording decodes")
        };
        assert_eq!(full.sample_rate, 24_000);
        for keep in [mp3.len() / 2 + 1, mp3.len() - 3] {
            let cut = mp3[..keep].to_vec();
            let (speech, _, _) = dial_with(move |_| vec![audio_frame(&cut), turn_end()]);
            match synth(&speech, "Ciao Boss, il pannello è su main.") {
                Ok(audio) => assert!(
                    !audio.samples.is_empty() && audio.samples.len() < full.samples.len(),
                    "{} of {} samples",
                    audio.samples.len(),
                    full.samples.len()
                ),
                Err(SpeechError::Failed(_)) => {}
                Err(other) => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn bytes_that_are_not_mp3_are_a_typed_failure() {
        // Catches: a proxy error page or garbage decoding to Ok or panicking.
        let (speech, _, _) = dial_with(|_| vec![audio_frame(&[0xABu8; 4000]), turn_end()]);
        assert!(matches!(
            synth(&speech, "ciao"),
            Err(SpeechError::Failed(_))
        ));
    }

    #[test]
    fn two_joined_requests_decode_to_both_of_them() {
        // Catches: the decoder stopping at the second stream's header, so a long
        // reply is spoken only up to the first 3000 bytes of text.
        let mp3 = recorded_mp3();
        let one = {
            let m = mp3.clone();
            let (speech, _, _) = dial_with(move |_| vec![audio_frame(&m), turn_end()]);
            synth(&speech, "Ciao Boss, il pannello è su main.")
                .unwrap()
                .samples
                .len()
        };
        let text = "parola ".repeat(600);
        assert_eq!(split_text(&text).len(), 2);
        let (speech, dials, _) = dial_with(move |_| vec![audio_frame(&mp3), turn_end()]);
        let both = synth(&speech, &text).expect("joined").samples.len();
        assert_eq!(dials.load(Ordering::SeqCst), 2);
        assert!(both * 100 >= one * 2 * 95, "{both} samples for 2 x {one}");
    }

    #[test]
    fn a_failing_second_piece_stops_before_the_third() {
        // Catches: continuing to dial after a piece failed, and returning the
        // first pieces' audio as if the reply were whole.
        let text = "parola ".repeat(1300);
        assert!(split_text(&text).len() >= 3);
        let (speech, dials, _) = dial_with(|n| match n {
            0 => vec![audio_frame(&recorded_mp3()), turn_end()],
            _ => vec![Ok(Frame::Closed)],
        });
        assert!(synth(&speech, &text).is_err());
        assert_eq!(dials.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn every_language_the_picker_offers_has_a_default_voice() {
        // Catches: a language selectable in settings (WHISPER_LANGUAGES: en es fr
        // de it pt nl ja zh ko ru) with no Edge default, so its replies fail with
        // "No default Microsoft Edge voice" on a fresh install.
        for code in [
            "en", "es", "fr", "de", "it", "pt", "nl", "ja", "zh", "ko", "ru",
        ] {
            assert!(choose_voice(code, "").is_ok(), "{code}");
        }
    }

    #[test]
    fn a_stored_voice_of_another_language_falls_back_for_the_reply() {
        // Catches: Italian voice reading English after the conversation language changed.
        assert_eq!(
            choose_voice("en", "it-IT-IsabellaNeural").unwrap(),
            "en-US-AriaNeural"
        );
        assert_eq!(
            choose_voice("it", "it-IT-IsabellaNeural").unwrap(),
            "it-IT-IsabellaNeural"
        );
    }
}

/// Adversarial cases from the critic of 1357-7d37 (round 2: dial thread, escaped split).
#[cfg(test)]
mod critic_round2 {
    use super::critic_round1::{VOICE, audio_frame, dial_with, recorded_mp3, synth, turn_end};
    use super::*;
    use std::sync::atomic::Ordering;

    fn plain(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace()).collect()
    }

    #[test]
    fn an_abandoned_dial_neither_sends_nor_serves_the_next_reply() {
        // Catches: the late connection of a cancelled reply being picked up by
        // the next request (shared channel) or writing the old text to the service.
        let mp3 = recorded_mp3();
        let (speech, dials, sent) = dial_with(move |n| {
            if n == 0 {
                std::thread::sleep(Duration::from_millis(300));
            }
            vec![audio_frame(&mp3), turn_end()]
        });
        let cancel = SpeechCancel::new();
        let remote = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            remote.cancel();
        });
        let first = speech.synthesize("vecchio testo", VOICE, &cancel);
        assert!(matches!(first, Err(SpeechError::Cancelled)), "{first:?}");

        let second = synth(&speech, "nuovo testo").expect("the second reply is served");
        assert!(!second.samples.is_empty());
        std::thread::sleep(Duration::from_millis(500)); // the abandoned dial finishes
        let sent = sent.lock().unwrap();
        assert_eq!(dials.load(Ordering::SeqCst), 2);
        assert_eq!(
            sent.len(),
            2,
            "config + ssml of the second reply only: {sent:?}"
        );
        assert!(sent.iter().all(|m| !m.contains("vecchio")), "{sent:?}");
    }

    #[test]
    fn a_dial_that_panics_is_a_typed_failure_not_a_hang() {
        // Catches: the poll loop waiting forever on a channel whose sender died.
        let speech = EdgeSpeech::with_dial(Box::new(|| panic!("resolver blew up")));
        let start = Instant::now();
        let result = synth(&speech, "ciao");
        assert!(
            matches!(&result, Err(SpeechError::Failed(m)) if m.contains("cannot reach")),
            "{result:?}"
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn the_deadline_is_not_re_armed_after_a_slow_dial() {
        // Catches: the time limit restarting once connected, so a 200 ms dial plus
        // a 300 ms limit lets a mute service hold the reply for 500 ms.
        let speech = EdgeSpeech::with_dial(Box::new(|| {
            std::thread::sleep(Duration::from_millis(200));
            struct Mute;
            impl Socket for Mute {
                fn send(&mut self, _: String) -> Result<()> {
                    Ok(())
                }
                fn recv(&mut self) -> Result<Frame> {
                    std::thread::sleep(Duration::from_millis(10));
                    Ok(Frame::Idle)
                }
            }
            Ok(Box::new(Mute))
        }))
        .with_timeout(Duration::from_millis(300));
        let start = Instant::now();
        let result = synth(&speech, "ciao");
        assert!(
            matches!(&result, Err(SpeechError::Failed(m)) if m.contains("in time")),
            "{result:?}"
        );
        assert!(
            start.elapsed() < Duration::from_millis(450),
            "{:?}",
            start.elapsed()
        );
    }

    #[test]
    fn a_mixed_text_is_cut_into_pieces_that_fit_and_lose_nothing() {
        // Catches: a cut that drops or repeats text, splits an entity's source
        // character, or lets a piece exceed the escaped limit (apostrophes,
        // ampersands, angle brackets, accents, CJK, astral emoji).
        let tokens = [
            "l'altro",
            "&",
            "è",
            "日本語",
            "<b>",
            "😀",
            "x".repeat(50).leak(),
            "dell'uomo",
        ];
        let text: String = (0..3000)
            .map(|i| tokens[(i * 7) % tokens.len()])
            .collect::<Vec<_>>()
            .join(" ");
        let pieces = split_text(&text);
        assert!(pieces.len() > 3);
        for piece in &pieces {
            assert!(!piece.is_empty());
            assert!(
                escape_xml(piece).len() <= MAX_TEXT_BYTES,
                "{}",
                escape_xml(piece).len()
            );
        }
        assert_eq!(plain(&pieces.concat()), plain(&text));
    }

    #[test]
    fn a_token_with_no_whitespace_is_cut_by_character_and_kept_whole() {
        // Catches: no-whitespace input looping forever, panicking inside a
        // multi-byte character, or losing the tail.
        for unit in ["&", "'", "😀", "è", "<"] {
            let text = unit.repeat(5000);
            let pieces = split_text(&text);
            assert!(pieces.len() >= 2, "{unit}");
            assert!(
                pieces.iter().all(|p| escape_xml(p).len() <= MAX_TEXT_BYTES),
                "{unit}"
            );
            assert_eq!(pieces.concat(), text, "{unit}");
        }
    }

    #[test]
    fn the_limit_is_inclusive_at_exactly_the_escaped_maximum() {
        // Catches: an off-by-one at the boundary (needless extra request, or a
        // piece one byte over).
        let exact = "'".repeat(MAX_TEXT_BYTES / 6);
        assert_eq!(escape_xml(&exact).len(), MAX_TEXT_BYTES);
        assert_eq!(split_text(&exact), vec![exact.as_str()]);
        let over = "'".repeat(MAX_TEXT_BYTES / 6 + 1);
        assert_eq!(split_text(&over).len(), 2);
    }

    #[test]
    fn every_request_the_service_receives_fits_its_text_limit() {
        // Catches: split and ssml_message disagreeing end to end: what is sent,
        // not what is cut, is what the service limits.
        let mp3 = recorded_mp3();
        let (speech, dials, sent) = dial_with(move |_| vec![audio_frame(&mp3), turn_end()]);
        let text = "Dell'uomo & l'altro ".repeat(600);
        let result = synth(&speech, &text);
        assert!(result.is_ok(), "{result:?}");
        assert!(dials.load(Ordering::SeqCst) >= 4);
        for message in sent
            .lock()
            .unwrap()
            .iter()
            .filter(|m| m.contains("Path:ssml"))
        {
            let open = message.find("<prosody").unwrap();
            let body = &message[open + message[open..].find('>').unwrap() + 1..];
            let body = body.trim_end_matches("</prosody></voice></speak>");
            assert!(body.len() <= 4096, "{} bytes of SSML text", body.len());
        }
    }

    /// Real TTS speech through the shipping canceller (1376-f33e). The
    /// recorded Edge reply is the far end; the same audio, 40 ms late and at
    /// 0.35 gain, is what the microphone hears. Catches: echo of a reply
    /// opening the voice gate, which hushed hands-free replies 600-900 ms in
    /// — and, over the same room, a user talking over it still being heard.
    #[test]
    fn recorded_speech_heard_back_is_not_a_voice_and_a_user_over_it_is() {
        let floor = crate::continuous::SegmenterConfig::default().activity_rms;
        let (echo, user) = echo_and_user_over_it_heard_at(floor);
        assert!(!echo, "the reply heard back counted as a voice");
        assert!(user, "a user talking over the reply was not heard");
    }

    /// (the reply heard back counts as a voice, a user over it does) when the
    /// segmenter's frame floor is `activity_rms`.
    fn echo_and_user_over_it_heard_at(activity_rms: f32) -> (bool, bool) {
        heard_through(activity_rms, || {
            Box::new(crate::echo::webrtc::WebRtc::new().expect("the APM starts"))
        })
    }

    /// The same, with the canceller `canceller` builds for each run.
    fn heard_through(
        activity_rms: f32,
        canceller: impl Fn() -> Box<dyn crate::echo::Canceller>,
    ) -> (bool, bool) {
        use crate::continuous::{Segmenter, SegmenterConfig};
        use crate::echo::{EchoGuard, SAMPLE_RATE};

        let reply = decode_mp3(recorded_mp3()).expect("the recorded stream decodes");
        let ratio = f64::from(SAMPLE_RATE) / f64::from(reply.sample_rate);
        let one: Vec<f32> = (0..(reply.samples.len() as f64 * ratio) as usize)
            .map(|n| {
                let at = n as f64 / ratio;
                let left = (at.floor() as usize).min(reply.samples.len() - 1);
                let right = (left + 1).min(reply.samples.len() - 1);
                let fraction = (at - left as f64) as f32;
                reply.samples[left] * (1.0 - fraction) + reply.samples[right] * fraction
            })
            .collect();
        // A long reply, as the real ones are: the canceller has to hold.
        let mut far: Vec<f32> = Vec::new();
        for _ in 0..4 {
            far.extend_from_slice(&one);
        }

        let run = |user: Option<std::ops::Range<usize>>| -> bool {
            let mut guard = EchoGuard::new(canceller());
            guard.note_rendered(&SpeechAudio {
                samples: far.clone(),
                sample_rate: SAMPLE_RATE,
            });
            let delay = SAMPLE_RATE as usize * 40 / 1_000;
            let total = far.len() + delay;
            let near: Vec<f32> = (0..total)
                .map(|n| {
                    let echo = n
                        .checked_sub(delay)
                        .and_then(|i| far.get(i))
                        .unwrap_or(&0.0)
                        * 0.35;
                    let voice =
                        user.as_ref()
                            .filter(|range| range.contains(&n))
                            .map_or(0.0, |_| {
                                0.5 * (2.0 * std::f32::consts::PI * 220.0 * n as f32
                                    / SAMPLE_RATE as f32)
                                    .sin()
                            });
                    echo + voice
                })
                .collect();
            let mut segmenter = Segmenter::new(SegmenterConfig {
                activity_rms,
                ..SegmenterConfig::default()
            });
            let mut heard = false;
            for chunk in near.chunks(SAMPLE_RATE as usize / 20) {
                segmenter.push(&guard.clean(chunk));
                heard |= segmenter.has_voice();
            }
            heard
        };

        let from = one.len() * 2;
        (run(None), run(Some(from..from + SAMPLE_RATE as usize)))
    }

    /// The Settings floor (1164-ee4b) is 0.001 for hands-free, ten times the
    /// old compiled one. Catches: residual echo clearing the 1376 voice gate
    /// at that floor and hushing a reply the user never interrupted. Prints
    /// the sweep so the playback-time floor can be chosen from numbers.
    #[test]
    fn the_settings_floor_still_keeps_a_reply_heard_back_from_being_a_voice() {
        for floor in [0.001, 0.002, 0.005, 0.01] {
            let (echo, user) = echo_and_user_over_it_heard_at(floor);
            eprintln!("floor {floor}: echo counts as voice = {echo}, user over it heard = {user}");
        }
        let floor = crate::transcribe::DEFAULT_RMS_THRESHOLD;
        // Negative control: with no cancellation the same echo must clear the
        // gate, or the assertion below proves nothing about the fixture.
        let (uncancelled, _) = heard_through(floor, || Box::new(crate::echo::PassThrough));
        assert!(
            uncancelled,
            "without a canceller the echo did not count as a voice at {floor}: the fixture does not exercise the gate"
        );
        let (echo, user) = echo_and_user_over_it_heard_at(floor);
        assert!(!echo, "the reply heard back counted as a voice at {floor}");
        assert!(
            user,
            "a user talking over the reply was not heard at {floor}"
        );
    }
}
