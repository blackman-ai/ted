// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Built-in caps
//!
//! These caps are embedded in the binary and always available.

mod defaults;

pub use defaults::{get_builtin, list_builtins};
