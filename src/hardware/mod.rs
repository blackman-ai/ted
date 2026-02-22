// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Hardware detection and system profiling for adaptive behavior
//!
//! This module implements the "2010 Dell Benchmark" philosophy by detecting
//! system capabilities and adapting Ted's behavior accordingly.

pub mod detector;
pub mod thermal;
pub mod tier;
pub mod upgrade;

pub use detector::*;
pub use thermal::*;
pub use tier::*;
pub use upgrade::*;
