//! JSON Schema → protobuf message generator (paridad Node `jsonSchemaToProto.ts`).

use std::collections::{HashMap, HashSet};

use serde_json::Value;

pub type JsonSchema = Value;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct GeneratedMessage {
    pub name: String,
    pub lines: Vec<String>,
    pub uses_struct: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageMode {
    Empty,
    Struct,
    Schema,
}

#[derive(Debug, Default)]
pub struct ProtoSchemaGenerator {
    messages: HashMap<String, Vec<String>>,
    uses_struct: bool,
}

impl ProtoSchemaGenerator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_uses_struct(&self) -> bool {
        if self.uses_struct {
            return true;
        }
        self.get_messages()
            .iter()
            .any(|message| message.uses_struct)
    }

    pub fn get_messages(&self) -> Vec<GeneratedMessage> {
        self.messages
            .iter()
            .map(|(name, lines)| GeneratedMessage {
                name: name.clone(),
                lines: lines.clone(),
                uses_struct: lines
                    .iter()
                    .any(|line| line.contains("google.protobuf.Struct")),
            })
            .collect()
    }

    pub fn generate_message(
        &mut self,
        name: &str,
        schema: Option<&JsonSchema>,
        mode: MessageMode,
    ) {
        if self.messages.contains_key(name) {
            return;
        }

        if mode == MessageMode::Empty || schema.is_none() {
            self.messages
                .insert(name.to_string(), vec![format!("message {name} {{}}")]);
            return;
        }

        if mode == MessageMode::Struct {
            self.mark_uses_struct();
            self.messages.insert(
                name.to_string(),
                vec![
                    format!("message {name} {{"),
                    "  google.protobuf.Struct value = 1;".into(),
                    "}".into(),
                ],
            );
            return;
        }

        let lines = self.build_message_lines(name, schema.unwrap());
        self.messages.insert(name.to_string(), lines);
    }

    pub fn generate_array_wrapper_message(
        &mut self,
        response_name: &str,
        item_schema: &JsonSchema,
        item_message_name: &str,
    ) {
        if let Some(repeated_type) =
            self.resolve_scalar_or_enum_repeated_type(item_schema, &format!("{item_message_name}Enum"))
        {
            self.messages.insert(
                response_name.to_string(),
                vec![
                    format!("message {response_name} {{"),
                    format!("  repeated {repeated_type} items = 1;"),
                    "}".into(),
                ],
            );
            return;
        }

        self.generate_message(item_message_name, Some(item_schema), MessageMode::Schema);
        self.messages.insert(
            response_name.to_string(),
            vec![
                format!("message {response_name} {{"),
                format!("  repeated {item_message_name} items = 1;"),
                "}".into(),
            ],
        );
    }

    fn mark_uses_struct(&mut self) {
        self.uses_struct = true;
    }

    fn struct_field(&mut self, field_number: u32, snake_name: &str, optional: bool) -> String {
        self.mark_uses_struct();
        let optional_prefix = if optional { "optional " } else { "" };
        format!("{optional_prefix}google.protobuf.Struct {snake_name} = {field_number};")
    }

    fn build_message_lines(&mut self, message_name: &str, schema: &JsonSchema) -> Vec<String> {
        let resolved = resolve_json_schema(schema);

        match resolved {
            ResolvedSchema::Array { items, .. } => {
                let Some(items) = items else {
                    let field = self.struct_field(1, "items", false);
                    return vec![
                        format!("message {message_name} {{"),
                        format!("  {}", field.trim()),
                        "}".into(),
                    ];
                };

                if let Some(repeated_type) = self
                    .resolve_scalar_or_enum_repeated_type(&items, &format!("{message_name}ItemEnum"))
                {
                    return vec![
                        format!("message {message_name} {{"),
                        format!("  repeated {repeated_type} items = 1;"),
                        "}".into(),
                    ];
                }

                let item_message_name = format!("{message_name}Item");
                if !self.messages.contains_key(&item_message_name) {
                    let item_lines = self.build_message_lines(&item_message_name, &items);
                    self.messages.insert(item_message_name.clone(), item_lines);
                }

                vec![
                    format!("message {message_name} {{"),
                    format!("  repeated {item_message_name} items = 1;"),
                    "}".into(),
                ]
            }
            ResolvedSchema::Struct { .. } => {
                self.mark_uses_struct();
                vec![
                    format!("message {message_name} {{"),
                    "  google.protobuf.Struct value = 1;".into(),
                    "}".into(),
                ]
            }
            ResolvedSchema::Object {
                properties,
                required,
                ..
            } => {
                let mut lines = vec![format!("message {message_name} {{")];
                let mut field_number = 1u32;

                for (property_name, property_schema) in &properties {
                    let field = self.build_field(
                        property_name,
                        property_schema,
                        message_name,
                        field_number,
                        &required,
                    );
                    lines.push(format!("  {field}"));
                    field_number += 1;
                }

                lines.push("}".into());
                lines
            }
            _ => {
                self.mark_uses_struct();
                vec![
                    format!("message {message_name} {{"),
                    "  google.protobuf.Struct value = 1;".into(),
                    "}".into(),
                ]
            }
        }
    }

    fn build_field(
        &mut self,
        property_name: &str,
        property_schema: &JsonSchema,
        parent_message_name: &str,
        field_number: u32,
        required: &HashSet<String>,
    ) -> String {
        let snake_name = to_snake_case(property_name);
        let resolved = resolve_json_schema(property_schema);
        let optional_prefix =
            if required.contains(property_name) || !resolved.is_optional() {
                ""
            } else {
                "optional "
            };

        match resolved {
            ResolvedSchema::Array { items, optional } => {
                let Some(items) = items else {
                    return self.struct_field(
                        field_number,
                        &snake_name,
                        !required.contains(property_name) || optional,
                    );
                };

                if let Some(repeated_type) = self.resolve_scalar_or_enum_repeated_type(
                    &items,
                    &format!(
                        "{parent_message_name}{}ItemEnum",
                        to_pascal_case(property_name)
                    ),
                ) {
                    return format!(
                        "{optional_prefix}repeated {repeated_type} {snake_name} = {field_number};"
                    );
                }

                let nested_name = format!(
                    "{parent_message_name}{}Item",
                    to_pascal_case(property_name)
                );
                if !self.messages.contains_key(&nested_name) {
                    let nested_lines = self.build_message_lines(&nested_name, &items);
                    self.messages.insert(nested_name.clone(), nested_lines);
                }

                format!("{optional_prefix}repeated {nested_name} {snake_name} = {field_number};")
            }
            ResolvedSchema::Enum { enum_values, .. } => {
                let enum_name =
                    format!("{parent_message_name}{}Enum", to_pascal_case(property_name));
                self.generate_enum(&enum_name, &enum_values);
                format!("{optional_prefix}{enum_name} {snake_name} = {field_number};")
            }
            ResolvedSchema::Struct { .. } => {
                self.struct_field(field_number, &snake_name, !optional_prefix.is_empty())
            }
            ResolvedSchema::Object { .. } => {
                let nested_name =
                    format!("{parent_message_name}{}", to_pascal_case(property_name));
                if !self.messages.contains_key(&nested_name) {
                    let nested_lines = self.build_message_lines(&nested_name, property_schema);
                    self.messages.insert(nested_name.clone(), nested_lines);
                }
                format!("{optional_prefix}{nested_name} {snake_name} = {field_number};")
            }
            ResolvedSchema::Scalar { scalar_type, .. } => {
                let mapped = map_scalar_type(&scalar_type);
                if mapped == "google.protobuf.Struct" {
                    return self.struct_field(
                        field_number,
                        &snake_name,
                        !optional_prefix.is_empty(),
                    );
                }
                format!("{optional_prefix}{mapped} {snake_name} = {field_number};")
            }
        }
    }

    fn resolve_scalar_or_enum_repeated_type(
        &mut self,
        items: &JsonSchema,
        enum_name: &str,
    ) -> Option<String> {
        match resolve_json_schema(items) {
            ResolvedSchema::Scalar { scalar_type, .. } => {
                let mapped = map_scalar_type(&scalar_type);
                if mapped != "google.protobuf.Struct" {
                    Some(mapped)
                } else {
                    None
                }
            }
            ResolvedSchema::Enum { enum_values, .. } => {
                self.generate_enum(enum_name, &enum_values);
                Some(enum_name.to_string())
            }
            _ => None,
        }
    }

    fn generate_enum(&mut self, name: &str, values: &[Value]) {
        if self.messages.contains_key(name) {
            return;
        }

        let mut lines = vec![format!("enum {name} {{"), "  UNSPECIFIED = 0;".into()];
        let mut used_names: HashSet<String> = HashSet::from(["UNSPECIFIED".into()]);

        for value in values {
            let enum_key = to_proto_enum_identifier(&value_to_string(value));
            let mut candidate = enum_key.clone();
            let mut suffix = 1u32;
            while used_names.contains(&candidate) {
                candidate = format!("{enum_key}_{suffix}");
                suffix += 1;
            }
            used_names.insert(candidate.clone());
            lines.push(format!("  {candidate} = {};", used_names.len() - 1));
        }

        lines.push("}".into());
        self.messages.insert(name.to_string(), lines);
    }
}

#[derive(Debug)]
enum ResolvedSchema {
    Object {
        properties: Vec<(String, JsonSchema)>,
        required: HashSet<String>,
        optional: bool,
    },
    Array {
        items: Option<JsonSchema>,
        optional: bool,
    },
    Enum {
        enum_values: Vec<Value>,
        optional: bool,
    },
    Struct {
        optional: bool,
    },
    Scalar {
        scalar_type: JsonSchema,
        optional: bool,
    },
}

impl ResolvedSchema {
    fn is_optional(&self) -> bool {
        match self {
            Self::Object { optional, .. }
            | Self::Array { optional, .. }
            | Self::Enum { optional, .. }
            | Self::Struct { optional }
            | Self::Scalar { optional, .. } => *optional,
        }
    }
}

fn resolve_json_schema(schema: &JsonSchema) -> ResolvedSchema {
    let optional = is_nullable_schema(schema);

    if let Some(enum_values) = schema.get("enum").and_then(|v| v.as_array()) {
        return ResolvedSchema::Enum {
            enum_values: enum_values.clone(),
            optional,
        };
    }

    let normalized = if needs_normalize(schema) {
        normalize_schema_owned(schema)
    } else {
        schema.clone()
    };

    if normalized.get("type").and_then(|t| t.as_str()) == Some("array") {
        return ResolvedSchema::Array {
            items: normalized.get("items").cloned(),
            optional,
        };
    }

    if is_record_schema(&normalized) {
        return ResolvedSchema::Struct { optional };
    }

    let is_object = normalized.get("type").and_then(|t| t.as_str()) == Some("object")
        || normalized.get("properties").is_some();

    if is_object {
        let properties = normalized
            .get("properties")
            .and_then(|p| p.as_object())
            .map(|map| {
                map.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let required = normalized
            .get("required")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        return ResolvedSchema::Object {
            properties,
            required,
            optional,
        };
    }

    if is_scalar_schema(&normalized) {
        return ResolvedSchema::Scalar {
            scalar_type: normalized,
            optional,
        };
    }

    ResolvedSchema::Struct { optional }
}

fn needs_normalize(schema: &JsonSchema) -> bool {
    schema.get("type").map(|t| t.is_array()).unwrap_or(false)
        || schema.get("anyOf").is_some()
        || schema.get("oneOf").is_some()
}

fn normalize_schema_owned(schema: &JsonSchema) -> JsonSchema {
    if let Some(types) = schema.get("type").and_then(|t| t.as_array()) {
        let filtered: Vec<_> = types
            .iter()
            .filter(|entry| entry.as_str() != Some("null"))
            .cloned()
            .collect();
        if filtered.len() == 1 {
            let mut next = schema.clone();
            next["type"] = filtered[0].clone();
            return normalize_schema_owned(&next);
        }
    }

    if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
        return normalize_union(any_of, schema);
    }

    if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
        return normalize_union(one_of, schema);
    }

    schema.clone()
}

fn normalize_union(options: &[Value], parent: &JsonSchema) -> JsonSchema {
    let non_null: Vec<&Value> = options.iter().filter(|option| !is_null_schema(option)).collect();

    if non_null.len() == 1 {
        return normalize_schema_owned(non_null[0]);
    }

    if non_null.iter().all(|option| is_string_like_schema(option)) {
        return serde_json::json!({ "type": "string" });
    }

    if non_null.iter().all(|option| {
        matches!(
            option.get("type").and_then(|t| t.as_str()),
            Some("number") | Some("integer")
        )
    }) {
        let has_number = non_null
            .iter()
            .any(|option| option.get("type").and_then(|t| t.as_str()) == Some("number"));
        return serde_json::json!({
            "type": if has_number { "number" } else { "integer" }
        });
    }

    if non_null
        .iter()
        .all(|option| option.get("type").and_then(|t| t.as_str()) == Some("boolean"))
    {
        return serde_json::json!({ "type": "boolean" });
    }

    if let Some(scalar) = non_null.iter().find(|option| is_scalar_schema(option)) {
        return normalize_schema_owned(scalar);
    }

    parent.clone()
}

fn is_nullable_schema(schema: &JsonSchema) -> bool {
    if let Some(types) = schema.get("type").and_then(|t| t.as_array()) {
        if types.iter().any(|t| t.as_str() == Some("null")) {
            return true;
        }
    }

    if let Some(any_of) = schema.get("anyOf").and_then(|v| v.as_array()) {
        if any_of.iter().any(is_null_schema) {
            return true;
        }
    }

    if let Some(one_of) = schema.get("oneOf").and_then(|v| v.as_array()) {
        if one_of.iter().any(is_null_schema) {
            return true;
        }
    }

    false
}

fn is_null_schema(schema: &JsonSchema) -> bool {
    if schema.get("type").and_then(|t| t.as_str()) == Some("null") {
        return true;
    }
    if let Some(types) = schema.get("type").and_then(|t| t.as_array()) {
        return types.len() == 1 && types[0].as_str() == Some("null");
    }
    false
}

fn is_string_like_schema(schema: &JsonSchema) -> bool {
    if schema.get("type").and_then(|t| t.as_str()) == Some("string") {
        return true;
    }
    if schema.get("type").and_then(|t| t.as_str()) == Some("object")
        && schema.get("format").and_then(|f| f.as_str()) == Some("date-time")
    {
        return true;
    }
    matches!(
        schema.get("format").and_then(|f| f.as_str()),
        Some("date-time") | Some("date") | Some("time")
    )
}

fn is_record_schema(schema: &JsonSchema) -> bool {
    let has_additional = schema
        .get("additionalProperties")
        .map(|v| v != &Value::Bool(false))
        .unwrap_or(false);
    if schema.get("type").and_then(|t| t.as_str()) != Some("object") && !has_additional {
        return false;
    }

    let has_properties = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|m| !m.is_empty())
        .unwrap_or(false);

    has_additional && !has_properties
}

fn is_scalar_schema(schema: &JsonSchema) -> bool {
    matches!(
        schema.get("type").and_then(|t| t.as_str()),
        Some("string") | Some("integer") | Some("number") | Some("boolean")
    )
}

fn map_scalar_type(schema: &JsonSchema) -> String {
    if let Some(const_val) = schema.get("const") {
        return match const_val {
            Value::Bool(_) => "bool".into(),
            Value::Number(_) => "double".into(),
            _ => "string".into(),
        };
    }

    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => "string".into(),
        Some("integer") => "int64".into(),
        Some("number") => "double".into(),
        Some("boolean") => "bool".into(),
        _ => "google.protobuf.Struct".into(),
    }
}

fn to_snake_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    let chars: Vec<char> = value.chars().collect();
    for (i, ch) in chars.iter().enumerate() {
        if *ch == '-' || ch.is_whitespace() {
            if !out.ends_with('_') {
                out.push('_');
            }
            continue;
        }
        if ch.is_ascii_uppercase() {
            if i > 0 {
                let prev = chars[i - 1];
                if prev.is_ascii_lowercase() || prev.is_ascii_digit() {
                    out.push('_');
                }
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(*ch);
        }
    }
    out.trim_matches('_').to_string()
}

fn to_pascal_case(value: &str) -> String {
    let mut out = String::new();
    let mut capitalize = true;
    for ch in value.chars() {
        if ch == '_' || ch == '-' || ch.is_whitespace() {
            capitalize = true;
            continue;
        }
        if !ch.is_ascii_alphanumeric() {
            continue;
        }
        if capitalize {
            out.extend(ch.to_uppercase());
            capitalize = false;
        } else {
            out.push(ch);
        }
    }
    out
}

fn to_proto_enum_identifier(value: &str) -> String {
    let mut normalized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    while normalized.starts_with('_') {
        normalized.remove(0);
    }
    while normalized.ends_with('_') {
        normalized.pop();
    }
    if normalized
        .chars()
        .next()
        .map(|c| c.is_ascii_digit())
        .unwrap_or(false)
    {
        normalized.insert(0, '_');
    }
    if normalized.is_empty() {
        "VALUE".into()
    } else {
        normalized
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Convert a JSON Schema into a protobuf message on `generator`.
/// Returns the mode used (`empty` / `struct` / `schema`).
pub fn schema_to_proto_message(
    generator: &mut ProtoSchemaGenerator,
    message_name: &str,
    schema: Option<&JsonSchema>,
) -> MessageMode {
    let Some(schema) = schema else {
        generator.generate_message(message_name, None, MessageMode::Empty);
        return MessageMode::Empty;
    };

    generator.generate_message(message_name, Some(schema), MessageMode::Schema);
    MessageMode::Schema
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_schema_to_message() {
        let mut generator = ProtoSchemaGenerator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "user": { "type": "string" },
                "password": { "type": "string" }
            },
            "required": ["user", "password"]
        });
        assert_eq!(
            schema_to_proto_message(&mut generator, "passwordRequest", Some(&schema)),
            MessageMode::Schema
        );
        let text = generator
            .get_messages()
            .iter()
            .find(|m| m.name == "passwordRequest")
            .unwrap()
            .lines
            .join("\n");
        assert!(text.contains("string user = 1;"));
        assert!(text.contains("string password = 2;"));
    }

    #[test]
    fn enum_preserves_identifiers() {
        let mut generator = ProtoSchemaGenerator::new();
        let schema = json!({
            "type": "object",
            "properties": {
                "role": { "enum": ["corporation_user", "admin"] }
            }
        });
        schema_to_proto_message(&mut generator, "RoleRequest", Some(&schema));
        let joined = generator
            .get_messages()
            .iter()
            .flat_map(|m| m.lines.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("corporation_user"));
        assert!(joined.contains("admin"));
    }

    #[test]
    fn array_output_wrapper() {
        let mut generator = ProtoSchemaGenerator::new();
        let item = json!({ "type": "string" });
        generator.generate_array_wrapper_message("listResponse", &item, "listResponseItem");
        let text = generator
            .get_messages()
            .iter()
            .find(|m| m.name == "listResponse")
            .unwrap()
            .lines
            .join("\n");
        assert!(text.contains("repeated string items = 1;"));
    }

    #[test]
    fn empty_schema_is_empty_message() {
        let mut generator = ProtoSchemaGenerator::new();
        assert_eq!(
            schema_to_proto_message(&mut generator, "EmptyRequest", None),
            MessageMode::Empty
        );
        assert_eq!(
            generator.get_messages()[0].lines,
            vec!["message EmptyRequest {}".to_string()]
        );
    }
}
