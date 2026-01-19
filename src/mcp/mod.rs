// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Model Context Protocol (MCP) server implementation
//!
//! Exposes Ted's tools to MCP-compatible clients like Claude Desktop

pub mod protocol;
pub mod server;
pub mod transport;

pub use protocol::*;
pub use server::*;
pub use transport::*;
