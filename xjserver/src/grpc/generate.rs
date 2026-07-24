//! Generate `.proto` text from a [`RouteRegistry`] (paridad Node `generateProtoFromRegistry`).

use std::collections::{HashMap, HashSet};

use crate::error::XJError;
use crate::grpc::proto::{MessageMode, ProtoSchemaGenerator, schema_to_proto_message};
use crate::grpc::types::{DEFAULT_PROTO_PACKAGE, GeneratedProto, RouteProtoShape};
use crate::registry::{RouteBucket, RouteRegistry};

const BUCKET_SERVICES: [RouteBucket; 4] = [
    RouteBucket::Xj,
    RouteBucket::Login,
    RouteBucket::Session,
    RouteBucket::Logout,
];

#[derive(Debug, Clone, Default)]
pub struct GenerateProtoOptions {
    pub package_name: Option<String>,
}

/// Build a protobuf definition from registered routes and their JSON Schemas.
pub fn generate_proto_from_registry(
    registry: &RouteRegistry,
    options: GenerateProtoOptions,
) -> Result<GeneratedProto, XJError> {
    let package_name = options
        .package_name
        .unwrap_or_else(|| DEFAULT_PROTO_PACKAGE.to_string());

    let mut route_names = HashSet::new();
    let mut generator = ProtoSchemaGenerator::new();
    let mut shapes = Vec::new();
    let mut services: HashMap<RouteBucket, Vec<String>> = HashMap::from([
        (RouteBucket::Xj, Vec::new()),
        (RouteBucket::Login, Vec::new()),
        (RouteBucket::Session, Vec::new()),
        (RouteBucket::Logout, Vec::new()),
    ]);

    for bucket in BUCKET_SERVICES {
        for route in registry.routes_in_bucket(bucket) {
            let route_name = route.name();
            if !route_names.insert(route_name) {
                return Err(XJError::internal(format!(
                    "[XJServer] Duplicate route name '{route_name}' across buckets — proto generation requires globally unique route names"
                )));
            }

            services
                .get_mut(&bucket)
                .expect("bucket key")
                .push(route_name.to_string());

            shapes.push(build_route_shape(
                &mut generator,
                bucket,
                route_name,
                route.input_json_schema().as_ref(),
                route.output_json_schema().as_ref(),
            ));
        }
    }

    let mut lines: Vec<String> = vec![
        "syntax = \"proto3\";".into(),
        String::new(),
        format!("package {package_name};"),
        String::new(),
    ];

    if generator.get_uses_struct() {
        lines.push("import \"google/protobuf/struct.proto\";".into());
        lines.push(String::new());
    }

    for bucket in BUCKET_SERVICES {
        let route_list = services.get(&bucket).map(Vec::as_slice).unwrap_or(&[]);
        if route_list.is_empty() {
            continue;
        }

        lines.push(format!("service {} {{", bucket.grpc_service_name()));
        for route_name in route_list {
            lines.push(format!(
                "  rpc {route_name}({route_name}Request) returns ({route_name}Response);"
            ));
        }
        lines.push("}".into());
        lines.push(String::new());
    }

    for message in generator.get_messages() {
        lines.extend(message.lines);
        lines.push(String::new());
    }

    let text = format!("{}\n", lines.join("\n").trim());

    Ok(GeneratedProto {
        text,
        package_name,
        shapes,
        services,
    })
}

fn build_route_shape(
    generator: &mut ProtoSchemaGenerator,
    bucket: RouteBucket,
    route_name: &str,
    input: Option<&serde_json::Value>,
    output: Option<&serde_json::Value>,
) -> RouteProtoShape {
    let request_name = format!("{route_name}Request");
    let response_name = format!("{route_name}Response");

    let input_mode = schema_to_proto_message(generator, &request_name, input);

    let mut array_output = false;
    let mut struct_output = false;

    if output
        .and_then(|o| o.get("type"))
        .and_then(|t| t.as_str())
        == Some("array")
    {
        let item_schema = output
            .and_then(|o| o.get("items"))
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "type": "object" }));
        let item_message_name = format!("{route_name}ResponseItem");
        generator.generate_array_wrapper_message(
            &response_name,
            &item_schema,
            &item_message_name,
        );
        array_output = true;
    } else {
        let output_mode = schema_to_proto_message(generator, &response_name, output);
        struct_output = output_mode == MessageMode::Struct;
    }

    RouteProtoShape {
        route_name: route_name.to_string(),
        bucket,
        array_output,
        struct_input: input_mode == MessageMode::Struct,
        struct_output,
    }
}
