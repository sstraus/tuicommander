# Error Classification

**Module:** `src-tauri/src/error_classification.rs`

Classifies terminal error messages and calculates exponential backoff delays for retry logic.

## Tauri Commands

| Command | Signature | Description |
|---------|-----------|-------------|
| `classify_error_message` | `(message: String) -> String` | Classify error type |
| `calculate_backoff_delay_cmd` | `(retry_count, base_delay_ms, max_delay_ms, backoff_multiplier) -> f64` | Calculate backoff delay |

## Error Categories

`classify_error(message)` returns one of:

| Category | Description | Examples |
|----------|-------------|----------|
| `"transient"` | Temporary, retry-safe | Network timeout, connection reset, rate limit |
| `"permanent"` | Will not resolve with retry | Auth failure, not found, invalid input |
| `"unknown"` | Unclassified | Default for unrecognized patterns |

## Backoff Calculation

```rust
pub fn calculate_backoff_delay(
    retry_count: u32,
    base_delay_ms: f64,
    max_delay_ms: f64,
    backoff_multiplier: f64,
) -> f64
```

Formula: `min(base_delay_ms * backoff_multiplier^retry_count, max_delay_ms)`

Typical usage:
- `base_delay_ms`: 1000 (1 second)
- `max_delay_ms`: 30000 (30 seconds)
- `backoff_multiplier`: 2.0 (double each retry)

Result sequence: 1s → 2s → 4s → 8s → 16s → 30s → 30s → ...

## Frontend Integration

The `errorHandlingStore` calls these Rust functions to classify errors detected in terminal output and calculate retry delays. The store manages active retries and respects the user's configured strategy (retry, ignore, or manual).
