// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use reqwest::header::{HeaderMap, RETRY_AFTER};

use crate::error::{ApiError, TedError};

/// Parse token counts from an arbitrary message by extracting the first numeric tokens.
pub(crate) fn parse_numeric_token_counts(message: &str) -> (u32, u32) {
    let numbers: Vec<u32> = message
        .split(|c: char| !c.is_ascii_digit())
        .filter_map(|s| s.parse().ok())
        .collect();

    match numbers.as_slice() {
        [current, limit, ..] => (*current, *limit),
        [single] => (*single, 0),
        _ => (0, 0),
    }
}

/// Parse numeric Retry-After header (seconds).
pub(crate) fn parse_retry_after_seconds(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
}

/// Construct a standardized server error.
pub(crate) fn server_error(status: u16, message: impl Into<String>) -> TedError {
    TedError::Api(ApiError::ServerError {
        status,
        message: message.into(),
    })
}
