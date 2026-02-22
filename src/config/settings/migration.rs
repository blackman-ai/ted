// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

use serde_json::Value;

pub(super) fn migrate_on_load(value: Value) -> Value {
    // Placeholder migration hook. Keep identity transform until a schema migration
    // is required; centralizing this now prevents load/save logic from spreading.
    value
}

/// Deep-merge two JSON values.
/// `base` is existing file content, `overlay` is serialized current struct.
/// Overlay values take priority.
pub(super) fn deep_merge(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (key, overlay_val) in overlay_map {
                let merged = if let Some(base_val) = base_map.remove(&key) {
                    deep_merge(base_val, overlay_val)
                } else {
                    overlay_val
                };
                base_map.insert(key, merged);
            }
            Value::Object(base_map)
        }
        (_base, overlay) => overlay,
    }
}
