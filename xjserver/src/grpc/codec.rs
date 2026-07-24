//! gRPC payload codec (paridad Node `grpcPayloadCodec.ts`).

use serde_json::{Map, Value};

use crate::grpc::types::RouteProtoShape;

/// Unwrap Struct/array wrappers from a decoded protobuf JSON object.
pub fn decode_grpc_request(body: Value, shape: &RouteProtoShape) -> Value {
    if shape.struct_input {
        if let Value::Object(map) = &body {
            if map.contains_key("value") {
                return map.get("value").cloned().unwrap_or(Value::Object(Map::new()));
            }
        }
    }
    if body.is_null() {
        Value::Object(Map::new())
    } else {
        body
    }
}

/// Wrap response body for Struct/array protobuf messages.
pub fn encode_grpc_response(body: Value, shape: &RouteProtoShape) -> Value {
    if shape.array_output {
        let items = match body {
            Value::Array(arr) => arr,
            _ => Vec::new(),
        };
        return serde_json::json!({ "items": items });
    }

    if shape.struct_output {
        return serde_json::json!({ "value": body });
    }

    if body.is_null() {
        Value::Object(Map::new())
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::RouteBucket;

    fn shape(struct_in: bool, struct_out: bool, array_out: bool) -> RouteProtoShape {
        RouteProtoShape {
            route_name: "test".into(),
            bucket: RouteBucket::Xj,
            array_output: array_out,
            struct_input: struct_in,
            struct_output: struct_out,
        }
    }

    #[test]
    fn unwraps_struct_input() {
        let body = serde_json::json!({ "value": { "a": 1 } });
        let out = decode_grpc_request(body, &shape(true, false, false));
        assert_eq!(out, serde_json::json!({ "a": 1 }));
    }

    #[test]
    fn wraps_struct_output() {
        let body = serde_json::json!({ "a": 1 });
        let out = encode_grpc_response(body, &shape(false, true, false));
        assert_eq!(out, serde_json::json!({ "value": { "a": 1 } }));
    }

    #[test]
    fn wraps_array_output() {
        let body = serde_json::json!([1, 2]);
        let out = encode_grpc_response(body, &shape(false, false, true));
        assert_eq!(out, serde_json::json!({ "items": [1, 2] }));
    }
}
