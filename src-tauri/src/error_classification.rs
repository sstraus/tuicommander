/// Error classification for agent error messages.
///
/// This is the single source of truth for error pattern matching.
/// The frontend mirrors these patterns for synchronous classification.
///
/// Classify an error message into a known error type.
///
/// Returns one of: "rate_limit", "server", "network", "auth", "validation", "unknown".
#[allow(dead_code)] // Source-of-truth classifier; frontend mirrors these patterns
pub(crate) fn classify_error(message: &str) -> &'static str {
    let lower = message.to_lowercase();

    // Rate limit patterns
    if lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("quota exceeded")
        || lower.contains("429")
    {
        return "rate_limit";
    }

    // Server error patterns (5xx, API errors)
    // Use word-boundary-aware checks for numeric codes to avoid matching "5000ms", "15001", etc.
    if lower.contains("internal server error")
        || lower.contains("api_error")
        || lower.contains("service unavailable")
        || lower.contains("overloaded")
    {
        return "server";
    }
    // HTTP status codes — require surrounding context (space, punctuation, or line boundary)
    {
        lazy_static::lazy_static! {
            static ref HTTP_5XX: regex::Regex =
                regex::Regex::new(r"(?i)\b50[023]\b").unwrap();
        }
        if HTTP_5XX.is_match(&lower) {
            return "server";
        }
    }

    // Network patterns
    if lower.contains("network error")
        || lower.contains("connection refused")
        || lower.contains("timeout")
        || lower.contains("econnrefused")
        || lower.contains("etimedout")
    {
        return "network";
    }

    // Auth patterns
    if lower.contains("unauthorized")
        || lower.contains("authentication failed")
        || lower.contains("invalid api key")
    {
        return "auth";
    }

    // Validation patterns
    if lower.contains("invalid request") || lower.contains("validation error") {
        return "validation";
    }

    "unknown"
}

/// Calculate delay with exponential backoff.
///
/// Formula: min(base_delay_ms * multiplier^retry_count + jitter, max_delay_ms)
/// Jitter adds 10% random variation: delay * 0.1 * (random - 0.5)
///
/// This is the source of truth for the backoff algorithm.
/// The frontend mirrors this in `src/error-handler.ts:calculateBackoffDelay`.
pub(crate) fn calculate_backoff_delay(
    retry_count: u32,
    base_delay_ms: f64,
    max_delay_ms: f64,
    backoff_multiplier: f64,
) -> f64 {
    // Cap delay at max before computing jitter to avoid infinity/NaN arithmetic
    let delay = (base_delay_ms * backoff_multiplier.powi(retry_count as i32)).min(max_delay_ms);
    let jitter = delay * 0.1 * (rand::random::<f64>() - 0.5);
    (delay + jitter).min(max_delay_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_rate_limit_errors() {
        assert_eq!(classify_error("rate limit exceeded"), "rate_limit");
        assert_eq!(classify_error("Too Many Requests"), "rate_limit");
        assert_eq!(classify_error("quota exceeded"), "rate_limit");
        assert_eq!(classify_error("Error 429"), "rate_limit");
    }

    #[test]
    fn classifies_server_errors() {
        assert_eq!(classify_error("Internal server error"), "server");
        assert_eq!(classify_error("api_error: something broke"), "server");
        assert_eq!(classify_error("HTTP 500"), "server");
        assert_eq!(classify_error("Error 502 Bad Gateway"), "server");
        assert_eq!(classify_error("503 Service Unavailable"), "server");
        assert_eq!(classify_error("service unavailable"), "server");
        assert_eq!(classify_error("The server is overloaded"), "server");
    }

    #[test]
    fn classifies_network_errors() {
        assert_eq!(classify_error("network error"), "network");
        assert_eq!(classify_error("connection refused"), "network");
        assert_eq!(classify_error("request timeout"), "network");
        assert_eq!(classify_error("ECONNREFUSED"), "network");
        assert_eq!(classify_error("ETIMEDOUT"), "network");
    }

    #[test]
    fn classifies_auth_errors() {
        assert_eq!(classify_error("unauthorized access"), "auth");
        assert_eq!(classify_error("authentication failed"), "auth");
        assert_eq!(classify_error("invalid api key"), "auth");
    }

    #[test]
    fn classifies_validation_errors() {
        assert_eq!(classify_error("invalid request body"), "validation");
        assert_eq!(classify_error("validation error on field X"), "validation");
    }

    #[test]
    fn returns_unknown_for_unrecognized_errors() {
        assert_eq!(classify_error("something went wrong"), "unknown");
        assert_eq!(classify_error(""), "unknown");
    }

    #[test]
    fn classify_returns_static_str() {
        assert_eq!(classify_error("rate limit"), "rate_limit");
        assert_eq!(classify_error("unknown issue"), "unknown");
    }

    #[test]
    fn backoff_delay_exponential_growth() {
        // With base=1000, multiplier=2: delay doubles each retry (before jitter)
        // Jitter adds up to +/-5%, so we check within that range
        let d0 = calculate_backoff_delay(0, 1000.0, 30000.0, 2.0);
        assert!(d0 >= 950.0 && d0 <= 1050.0, "retry 0: got {d0}");

        let d1 = calculate_backoff_delay(1, 1000.0, 30000.0, 2.0);
        assert!(d1 >= 1900.0 && d1 <= 2100.0, "retry 1: got {d1}");

        let d2 = calculate_backoff_delay(2, 1000.0, 30000.0, 2.0);
        assert!(d2 >= 3800.0 && d2 <= 4200.0, "retry 2: got {d2}");

        let d3 = calculate_backoff_delay(3, 1000.0, 30000.0, 2.0);
        assert!(d3 >= 7600.0 && d3 <= 8400.0, "retry 3: got {d3}");
    }

    #[test]
    fn backoff_delay_caps_at_max() {
        // 1000 * 2^5 = 32000, should be capped at 30000
        let delay = calculate_backoff_delay(5, 1000.0, 30000.0, 2.0);
        assert!(delay <= 30000.0, "should cap at max: got {delay}");
    }

    #[test]
    fn backoff_delay_custom_config() {
        // base=500, multiplier=3, max=10000
        let d0 = calculate_backoff_delay(0, 500.0, 10000.0, 3.0);
        assert!(d0 >= 475.0 && d0 <= 525.0, "retry 0: got {d0}");

        let d1 = calculate_backoff_delay(1, 500.0, 10000.0, 3.0);
        assert!(d1 >= 1425.0 && d1 <= 1575.0, "retry 1: got {d1}");

        let d2 = calculate_backoff_delay(2, 500.0, 10000.0, 3.0);
        assert!(d2 >= 4275.0 && d2 <= 4725.0, "retry 2: got {d2}");

        // 500 * 3^3 = 13500, capped at 10000
        let d3 = calculate_backoff_delay(3, 500.0, 10000.0, 3.0);
        assert!(d3 <= 10000.0, "retry 3 should cap: got {d3}");
    }

    #[test]
    fn backoff_delay_large_retry_count_stays_capped() {
        let delay = calculate_backoff_delay(100, 1000.0, 5000.0, 2.0);
        assert!(delay <= 5000.0, "large retry should cap: got {delay}");
    }

    #[test]
    fn backoff_delay_zero_retry() {
        let delay = calculate_backoff_delay(0, 1000.0, 30000.0, 2.0);
        assert!(delay >= 950.0 && delay <= 1050.0, "retry 0 direct: got {delay}");
    }
}
