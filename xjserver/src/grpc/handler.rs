//! Dynamic gRPC route handler (paridad Node `createGrpcHandler`).

use std::collections::HashMap;
use std::sync::Arc;

use http::Extensions;
use prost_reflect::{
    DescriptorPool, DeserializeOptions, DynamicMessage, SerializeOptions,
};
use serde_json::Value;
use tonic::{Request, Response, Status, metadata::MetadataMap};

use crate::config::XJConfig;
use crate::context::ContextBase;
use crate::error::XJError;
use crate::grpc::codec::{decode_grpc_request, encode_grpc_response};
use crate::grpc::error::map_grpc_error;
use crate::grpc::types::RouteProtoShape;
use crate::login_token_middleware::LoginTokenMiddleware;
use crate::metadata::Metadata;
use crate::registry::{RouteBucket, RouteRegistry};
use crate::session_middleware::SessionMiddleware;

#[derive(Clone)]
pub struct GrpcHandlerState {
    pub registry: Arc<RouteRegistry>,
    pub config: Arc<XJConfig>,
    pub app_state: Arc<dyn std::any::Any + Send + Sync>,
    pub session_middleware: Arc<dyn SessionMiddleware>,
    pub login_token_middleware: Arc<dyn LoginTokenMiddleware>,
    pub pool: Arc<DescriptorPool>,
    pub package_name: String,
    pub shapes: Arc<HashMap<(RouteBucket, String), RouteProtoShape>>,
}

pub async fn handle_grpc_call(
    state: &GrpcHandlerState,
    bucket: RouteBucket,
    route_name: &str,
    request: Request<DynamicMessage>,
) -> Result<Response<DynamicMessage>, Status> {
    let shape = state
        .shapes
        .get(&(bucket, route_name.to_string()))
        .ok_or_else(|| Status::not_found(format!("Unknown route: {route_name}")))?
        .clone();

    let Some(route) = state.registry.get(bucket, route_name) else {
        let label = match bucket {
            RouteBucket::Xj => "Route",
            RouteBucket::Login => "Login route",
            RouteBucket::Session => "Session route",
            RouteBucket::Logout => "Logout route",
        };
        return Err(map_grpc_error(&XJError::not_found(format!(
            "{label} not found: {route_name}"
        ))));
    };

    let metadata = metadata_from_tonic(request.metadata());
    let session = state
        .session_middleware
        .resolve(&metadata, &state.config)
        .await
        .map_err(|err| map_grpc_error(&err))?;

    let request_json =
        dynamic_to_json(request.into_inner()).map_err(|err| map_grpc_error(&err))?;
    let body_value = decode_grpc_request(request_json, &shape);
    let body_bytes = serde_json::to_vec(&body_value)
        .map_err(|err| map_grpc_error(&XJError::bad_request(format!("Invalid JSON body: {err}"))))?;

    let base = ContextBase {
        session,
        metadata,
        state: state.app_state.clone(),
        config: state.config.clone(),
        extensions: Extensions::new(),
    };

    let outcome = route
        .dispatch(&body_bytes, base)
        .await
        .map_err(|err| map_grpc_error(&err))?;

    let mut trailing = MetadataMap::new();
    for (name, value) in outcome.metadata.outgoing() {
        if let (Ok(key), Ok(val)) = (
            name.parse::<tonic::metadata::MetadataKey<tonic::metadata::Ascii>>(),
            value.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>(),
        ) {
            trailing.insert(key, val);
        }
    }

    if bucket == RouteBucket::Login {
        match state
            .login_token_middleware
            .issue(
                &outcome.body,
                &outcome.metadata,
                &state.config,
                route_name,
            )
            .await
        {
            Ok(Some(token)) => {
                apply_auth_token_to_metadata(&mut trailing, &token, &state.config);
            }
            Ok(None) => {}
            Err(err) => return Err(map_grpc_error(&err)),
        }
    }

    let response_json = encode_grpc_response(outcome.body, &shape);
    let response_desc = state
        .pool
        .get_message_by_name(&format!(
            "{}.{route_name}Response",
            state.package_name
        ))
        .ok_or_else(|| Status::internal(format!("Missing response descriptor for {route_name}")))?;

    let response_msg =
        json_to_dynamic(response_desc, response_json).map_err(|err| map_grpc_error(&err))?;

    let mut response = Response::new(response_msg);
    *response.metadata_mut() = trailing;
    Ok(response)
}

fn metadata_from_tonic(map: &MetadataMap) -> Metadata {
    Metadata::from_header_iter(map.iter().filter_map(|key_and_value| match key_and_value {
        tonic::metadata::KeyAndValueRef::Ascii(key, value) => {
            let v = value.to_str().ok()?;
            Some((key.as_str(), v))
        }
        tonic::metadata::KeyAndValueRef::Binary(_, _) => None,
    }))
}

fn apply_auth_token_to_metadata(trailing: &mut MetadataMap, token: &str, config: &XJConfig) {
    let value = if config.token_prefix.is_empty() {
        token.to_string()
    } else {
        format!("{} {token}", config.token_prefix)
    };
    if let (Ok(key), Ok(val)) = (
        config
            .token_header
            .parse::<tonic::metadata::MetadataKey<tonic::metadata::Ascii>>(),
        value.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>(),
    ) {
        trailing.insert(key, val);
    }
}

fn dynamic_to_json(msg: DynamicMessage) -> Result<Value, XJError> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::new(&mut buf);
    msg.serialize_with_options(
        &mut ser,
        &SerializeOptions::new().use_proto_field_name(true),
    )
    .map_err(|err| XJError::bad_request(format!("Failed to decode protobuf: {err}")))?;
    serde_json::from_slice(&buf)
        .map_err(|err| XJError::bad_request(format!("Failed to decode protobuf JSON: {err}")))
}

fn json_to_dynamic(
    descriptor: prost_reflect::MessageDescriptor,
    value: Value,
) -> Result<DynamicMessage, XJError> {
    let json = value.to_string();
    let mut deserializer = serde_json::Deserializer::from_str(&json);
    DynamicMessage::deserialize_with_options(
        descriptor,
        &mut deserializer,
        &DeserializeOptions::new().deny_unknown_fields(false),
    )
    .map_err(|err| XJError::internal(format!("Failed to encode protobuf: {err}")))
}

/// Look up the request message descriptor for a route.
pub fn request_descriptor(
    pool: &DescriptorPool,
    package_name: &str,
    route_name: &str,
) -> Result<prost_reflect::MessageDescriptor, Status> {
    pool.get_message_by_name(&format!("{package_name}.{route_name}Request"))
        .ok_or_else(|| Status::internal(format!("Missing request descriptor for {route_name}")))
}
