use jsonschema::{Draft, Validator};
use schemars::{JsonSchema, schema_for};
use serde_json::Value;

use crate::error::{RouteValidationIssue, XJError};

/// JSON Schema for type `T` (via schemars).
pub fn json_schema_for<T: JsonSchema>() -> Value {
    serde_json::to_value(schema_for!(T)).expect("schema serialization")
}

/// Validate a parsed JSON value against `T`'s schema; returns structured issues on failure.
pub fn validate_json<T: JsonSchema>(value: &Value, route_name: &str) -> Result<(), XJError> {
    let schema = json_schema_for::<T>();
    let validator = Validator::options()
        .with_draft(Draft::Draft7)
        .build(&schema)
        .map_err(|err| {
            XJError::bad_request(format!("Invalid inputSchema for '{route_name}': {err}"))
        })?;

    let mut errors = validator.iter_errors(value);
    let first = errors.next();
    if let Some(first_err) = first {
        let mut issues = vec![RouteValidationIssue {
            path: first_err.instance_path.to_string(),
            message: first_err.to_string(),
            code: None,
        }];
        issues.extend(errors.map(|err| RouteValidationIssue {
            path: err.instance_path.to_string(),
            message: err.to_string(),
            code: None,
        }));
        return Err(XJError::ValidationBadRequest {
            message: format!("Validation failed for {route_name}"),
            issues,
        });
    }

    Ok(())
}
