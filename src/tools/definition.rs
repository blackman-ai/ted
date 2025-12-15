// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2025 Blackman Artificial Intelligence Technologies Inc.

//! Tool definition types
//!
//! These types are used to define tools for the LLM.

use serde_json::Value;

/// Helper to create a tool input schema
pub struct SchemaBuilder {
    properties: serde_json::Map<String, Value>,
    required: Vec<String>,
}

impl SchemaBuilder {
    /// Create a new schema builder
    pub fn new() -> Self {
        Self {
            properties: serde_json::Map::new(),
            required: vec![],
        }
    }

    /// Add a string property
    pub fn string(mut self, name: &str, description: &str, required: bool) -> Self {
        self.properties.insert(
            name.to_string(),
            serde_json::json!({
                "type": "string",
                "description": description
            }),
        );
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    /// Add an integer property
    pub fn integer(mut self, name: &str, description: &str, required: bool) -> Self {
        self.properties.insert(
            name.to_string(),
            serde_json::json!({
                "type": "integer",
                "description": description
            }),
        );
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    /// Add a boolean property
    pub fn boolean(mut self, name: &str, description: &str, required: bool) -> Self {
        self.properties.insert(
            name.to_string(),
            serde_json::json!({
                "type": "boolean",
                "description": description
            }),
        );
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    /// Add an array property
    pub fn array(mut self, name: &str, description: &str, item_type: &str, required: bool) -> Self {
        self.properties.insert(
            name.to_string(),
            serde_json::json!({
                "type": "array",
                "description": description,
                "items": {
                    "type": item_type
                }
            }),
        );
        if required {
            self.required.push(name.to_string());
        }
        self
    }

    /// Build the schema
    pub fn build(self) -> crate::llm::provider::ToolInputSchema {
        crate::llm::provider::ToolInputSchema {
            schema_type: "object".to_string(),
            properties: Value::Object(self.properties),
            required: self.required,
        }
    }
}

impl Default for SchemaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_builder_new() {
        let builder = SchemaBuilder::new();
        assert!(builder.properties.is_empty());
        assert!(builder.required.is_empty());
    }

    #[test]
    fn test_schema_builder_default() {
        let builder = SchemaBuilder::default();
        assert!(builder.properties.is_empty());
        assert!(builder.required.is_empty());
    }

    #[test]
    fn test_schema_builder_string_required() {
        let builder = SchemaBuilder::new().string("name", "The name field", true);

        assert!(builder.properties.contains_key("name"));
        assert!(builder.required.contains(&"name".to_string()));
    }

    #[test]
    fn test_schema_builder_string_optional() {
        let builder = SchemaBuilder::new().string("name", "The name field", false);

        assert!(builder.properties.contains_key("name"));
        assert!(!builder.required.contains(&"name".to_string()));
    }

    #[test]
    fn test_schema_builder_integer_required() {
        let builder = SchemaBuilder::new().integer("count", "The count field", true);

        assert!(builder.properties.contains_key("count"));
        assert!(builder.required.contains(&"count".to_string()));

        let prop = builder.properties.get("count").unwrap();
        assert_eq!(prop["type"], "integer");
    }

    #[test]
    fn test_schema_builder_integer_optional() {
        let builder = SchemaBuilder::new().integer("count", "The count field", false);

        assert!(!builder.required.contains(&"count".to_string()));
    }

    #[test]
    fn test_schema_builder_boolean_required() {
        let builder = SchemaBuilder::new().boolean("enabled", "Whether enabled", true);

        assert!(builder.properties.contains_key("enabled"));
        assert!(builder.required.contains(&"enabled".to_string()));

        let prop = builder.properties.get("enabled").unwrap();
        assert_eq!(prop["type"], "boolean");
    }

    #[test]
    fn test_schema_builder_boolean_optional() {
        let builder = SchemaBuilder::new().boolean("enabled", "Whether enabled", false);

        assert!(!builder.required.contains(&"enabled".to_string()));
    }

    #[test]
    fn test_schema_builder_array_required() {
        let builder = SchemaBuilder::new().array("items", "List of items", "string", true);

        assert!(builder.properties.contains_key("items"));
        assert!(builder.required.contains(&"items".to_string()));

        let prop = builder.properties.get("items").unwrap();
        assert_eq!(prop["type"], "array");
        assert_eq!(prop["items"]["type"], "string");
    }

    #[test]
    fn test_schema_builder_array_optional() {
        let builder = SchemaBuilder::new().array("items", "List of items", "integer", false);

        assert!(!builder.required.contains(&"items".to_string()));
    }

    #[test]
    fn test_schema_builder_chaining() {
        let builder = SchemaBuilder::new()
            .string("name", "Name field", true)
            .integer("age", "Age field", false)
            .boolean("active", "Is active", true)
            .array("tags", "Tags list", "string", false);

        assert_eq!(builder.properties.len(), 4);
        assert_eq!(builder.required.len(), 2);
        assert!(builder.required.contains(&"name".to_string()));
        assert!(builder.required.contains(&"active".to_string()));
    }

    #[test]
    fn test_schema_builder_build() {
        let schema = SchemaBuilder::new()
            .string("path", "File path", true)
            .integer("limit", "Max lines", false)
            .build();

        assert_eq!(schema.schema_type, "object");
        assert_eq!(schema.required.len(), 1);
        assert!(schema.required.contains(&"path".to_string()));

        if let Value::Object(props) = &schema.properties {
            assert!(props.contains_key("path"));
            assert!(props.contains_key("limit"));
        } else {
            panic!("Expected object properties");
        }
    }

    #[test]
    fn test_schema_builder_descriptions() {
        let builder = SchemaBuilder::new().string("name", "A description for name", true);

        let prop = builder.properties.get("name").unwrap();
        assert_eq!(prop["description"], "A description for name");
    }

    #[test]
    fn test_schema_builder_empty_build() {
        let schema = SchemaBuilder::new().build();

        assert_eq!(schema.schema_type, "object");
        assert!(schema.required.is_empty());

        if let Value::Object(props) = &schema.properties {
            assert!(props.is_empty());
        } else {
            panic!("Expected object properties");
        }
    }
}
