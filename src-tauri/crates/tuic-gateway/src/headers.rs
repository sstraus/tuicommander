//! Per-provider rate-limit header parsers that all produce one
//! [`BudgetObservation`]. Pure: no I/O, no clock reads.
//!
//! `Retry-After` is read as integer seconds only (all three providers send
//! seconds); an HTTP-date value is ignored.

use std::time::{Duration, SystemTime};

use http::HeaderMap;

use crate::model::{BucketId, LimitReason, Provider};

/// What one response says about the provider's budget.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BudgetObservation {
    /// Bucket named by the provider (`x-ratelimit-resource`, `RateLimit-Name`).
    pub bucket: Option<BucketId>,
    pub limit: Option<u32>,
    pub remaining: Option<u32>,
    pub reset: Option<SystemTime>,
    pub retry_after: Option<Duration>,
    /// `Some` when the provider refused the request or the bucket is empty.
    /// `PerIssue` and `Burst` are local to one issue or endpoint; only
    /// `Quota` and `Primary` pause everything.
    pub limited: Option<LimitReason>,
    /// Jira `X-RateLimit-NearLimit: true` (under 20 % capacity left).
    pub near_limit: bool,
    /// Jira Data Center token-bucket refill: `(requests, per interval)`.
    pub fill_rate: Option<(u32, Duration)>,
}

pub fn observe(
    provider: Provider,
    status: u16,
    headers: &HeaderMap,
    body_prefix: &[u8],
) -> BudgetObservation {
    match provider {
        Provider::GitHub => github(status, headers, body_prefix),
        Provider::GitLab => gitlab(status, headers),
        Provider::Jira => jira(status, headers),
        Provider::Generic => BudgetObservation {
            retry_after: retry_after(headers),
            limited: (status == 429).then_some(LimitReason::Unknown),
            ..Default::default()
        },
    }
}

fn github(status: u16, h: &HeaderMap, body: &[u8]) -> BudgetObservation {
    let mut o = BudgetObservation {
        bucket: text(h, "x-ratelimit-resource").map(|b| BucketId(b.to_owned())),
        limit: number(h, "x-ratelimit-limit"),
        remaining: number(h, "x-ratelimit-remaining"),
        reset: epoch(h, "x-ratelimit-reset"),
        retry_after: retry_after(h),
        ..Default::default()
    };
    let empty = o.remaining == Some(0);
    // GitHub names secondary limits only in the body (or by `retry-after`).
    let secondary = contains_ci(body, b"secondary rate limit");
    o.limited = match status {
        429 | 403 | 200 if secondary => Some(LimitReason::Secondary),
        // A 200 (GraphQL, or the last REST unit) with nothing left is exhaustion.
        _ if empty && matches!(status, 200..=299 | 403 | 429) => Some(LimitReason::Primary),
        429 => Some(LimitReason::Secondary),
        403 if o.retry_after.is_some() => Some(LimitReason::Secondary),
        _ => None,
    };
    o
}

fn gitlab(status: u16, h: &HeaderMap) -> BudgetObservation {
    let mut o = BudgetObservation {
        bucket: text(h, "ratelimit-name").map(|b| BucketId(b.to_owned())),
        limit: number(h, "ratelimit-limit"),
        remaining: number(h, "ratelimit-remaining"),
        reset: epoch(h, "ratelimit-reset"),
        retry_after: retry_after(h),
        ..Default::default()
    };
    // Projects/Groups/Users 429s omit the headers, so a bare 429 is `Unknown`.
    o.limited = if o.remaining == Some(0) {
        Some(LimitReason::Primary)
    } else {
        (status == 429).then_some(LimitReason::Unknown)
    };
    o
}

fn jira(status: u16, h: &HeaderMap) -> BudgetObservation {
    let interval = number(h, "x-ratelimit-interval-seconds");
    let fill = number(h, "x-ratelimit-fillrate");
    let mut o = BudgetObservation {
        limit: number(h, "x-ratelimit-limit"),
        remaining: number(h, "x-ratelimit-remaining"),
        reset: text(h, "x-ratelimit-reset").and_then(parse_iso8601),
        retry_after: retry_after(h),
        near_limit: text(h, "x-ratelimit-nearlimit") == Some("true"),
        fill_rate: fill
            .zip(interval)
            .map(|(n, s)| (n, Duration::from_secs(s.into()))),
        ..Default::default()
    };
    // Only a 429 is a refusal; a 503 with `Retry-After` is not a rate limit.
    if status == 429 {
        o.limited = Some(match text(h, "ratelimit-reason") {
            Some("jira-burst-based") => LimitReason::Burst,
            Some("jira-quota-global-based" | "jira-quota-tenant-based") => LimitReason::Quota,
            Some("jira-per-issue-on-write") => LimitReason::PerIssue,
            _ => LimitReason::Unknown,
        });
    }
    o
}

fn text<'a>(h: &'a HeaderMap, name: &str) -> Option<&'a str> {
    h.get(name)?.to_str().ok()
}

fn number(h: &HeaderMap, name: &str) -> Option<u32> {
    text(h, name)?.trim().parse().ok()
}

fn epoch(h: &HeaderMap, name: &str) -> Option<SystemTime> {
    let secs: u64 = text(h, name)?.trim().parse().ok()?;
    Some(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}

fn retry_after(h: &HeaderMap) -> Option<Duration> {
    let secs: u64 = text(h, "retry-after")?.trim().parse().ok()?;
    Some(Duration::from_secs(secs))
}

pub(crate) fn contains_ci(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle))
}

/// `YYYY-MM-DDTHH:MM:SS[.fff](Z|+hh:mm|-hh:mm)`; fractions are truncated.
fn parse_iso8601(s: &str) -> Option<SystemTime> {
    let bytes = s.as_bytes();
    let field = |from: usize, len: usize| -> Option<i64> {
        std::str::from_utf8(bytes.get(from..from + len)?)
            .ok()?
            .parse()
            .ok()
    };
    if bytes.len() < 20 || bytes[4] != b'-' || bytes[7] != b'-' || !matches!(bytes[10], b'T' | b't')
    {
        return None;
    }
    let (year, month, day) = (field(0, 4)?, field(5, 2)?, field(8, 2)?);
    let (hour, minute, second) = (field(11, 2)?, field(14, 2)?, field(17, 2)?);
    let mut at = 19;
    if bytes[at] == b'.' {
        at += 1;
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
    }
    let offset = match bytes.get(at)? {
        b'Z' | b'z' => 0,
        sign @ (b'+' | b'-') => {
            let secs = field(at + 1, 2)? * 3600 + field(at + 4, 2)? * 60;
            if *sign == b'+' { secs } else { -secs }
        }
        _ => return None,
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    // Days since 1970-01-01 (Howard Hinnant's civil-from-days inverse).
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let secs = days * 86_400 + hour * 3600 + minute * 60 + second - offset;
    u64::try_from(secs)
        .ok()
        .map(|secs| SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
}
