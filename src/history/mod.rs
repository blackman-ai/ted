// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! History management for Ted sessions
//!
//! Tracks session history including timestamps, working directories,
//! and conversation summaries for easy retrieval.

pub mod store;

pub use store::{HistoryStore, SessionInfo};
