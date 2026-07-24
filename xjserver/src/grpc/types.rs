use std::collections::HashMap;

use crate::registry::RouteBucket;

pub const DEFAULT_PROTO_PACKAGE: &str = "xjserver.v1";

/// Shape flags for encode/decode of a route's gRPC messages (paridad Node).
#[derive(Debug, Clone)]
pub struct RouteProtoShape {
    pub route_name: String,
    pub bucket: RouteBucket,
    pub array_output: bool,
    pub struct_input: bool,
    pub struct_output: bool,
}

/// Result of generating a protobuf definition from the route registry.
#[derive(Debug, Clone)]
pub struct GeneratedProto {
    pub text: String,
    pub package_name: String,
    pub shapes: Vec<RouteProtoShape>,
    pub services: HashMap<RouteBucket, Vec<String>>,
}
