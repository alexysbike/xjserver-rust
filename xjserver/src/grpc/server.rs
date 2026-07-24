//! Dynamic hyper HTTP/2 gRPC server (runtime proto via protox + prost-reflect).

use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::{BufMut, Bytes, BytesMut};
use futures_util::Future;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http2;
use hyper_util::rt::{TokioExecutor, TokioIo};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage};
use tokio::net::TcpListener;
use tonic::Status;
use tonic::body::BoxBody;
use tower::Service;

use crate::config::GrpcConfig;
use crate::error::XJError;
use crate::grpc::generate::{GenerateProtoOptions, generate_proto_from_registry};
use crate::grpc::handler::{GrpcHandlerState, handle_grpc_call, request_descriptor};
use crate::grpc::types::GeneratedProto;
use crate::grpc::write_proto::write_generated_proto;
use crate::http::HttpState;
use crate::registry::RouteBucket;

/// Start the gRPC server: generate proto, write file, bind and serve.
pub async fn start_grpc_server(
    http_state: HttpState,
    grpc: &GrpcConfig,
    addr: SocketAddr,
) -> Result<(), std::io::Error> {
    grpc.validate()
        .map_err(|msg| std::io::Error::new(std::io::ErrorKind::InvalidInput, msg))?;

    let generated = generate_proto_from_registry(
        &http_state.registry,
        GenerateProtoOptions {
            package_name: Some(grpc.package.clone()),
        },
    )
    .map_err(|err| std::io::Error::other(err.message().to_string()))?;

    write_generated_proto(&grpc.proto_path, &generated.text)
        .map_err(|err| std::io::Error::other(err.message().to_string()))?;

    let pool = compile_proto_pool(&grpc.proto_path).map_err(std::io::Error::other)?;

    let mut shapes = HashMap::new();
    for shape in &generated.shapes {
        shapes.insert((shape.bucket, shape.route_name.clone()), shape.clone());
    }

    let handler_state = GrpcHandlerState {
        registry: http_state.registry.clone(),
        config: http_state.config.clone(),
        app_state: http_state.app_state.clone(),
        session_middleware: http_state.session_middleware.clone(),
        login_token_middleware: http_state.login_token_middleware.clone(),
        pool: Arc::new(pool),
        package_name: generated.package_name.clone(),
        shapes: Arc::new(shapes),
    };

    let listener = TcpListener::bind(addr).await?;
    let max_recv = grpc.max_receive_message_length;
    let max_send = grpc.max_send_message_length;

    println!("gRPC server running on {addr}");

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let state = handler_state.clone();
        tokio::spawn(async move {
            let svc = GrpcRouterService {
                state,
                max_recv,
                max_send,
            };
            let hyper_svc = hyper::service::service_fn(move |req: http::Request<Incoming>| {
                let mut svc = svc.clone();
                async move { svc.call(req).await }
            });
            if let Err(err) = http2::Builder::new(TokioExecutor::new())
                .serve_connection(io, hyper_svc)
                .await
            {
                eprintln!("[XJServer] gRPC connection error: {err}");
            }
        });
    }
}

/// Generate proto + compile descriptor pool without serving (tests / manifest).
pub fn prepare_grpc(
    registry: &crate::registry::RouteRegistry,
    grpc: &GrpcConfig,
) -> Result<(GeneratedProto, DescriptorPool), XJError> {
    grpc.validate().map_err(XJError::internal)?;
    let generated = generate_proto_from_registry(
        registry,
        GenerateProtoOptions {
            package_name: Some(grpc.package.clone()),
        },
    )?;
    write_generated_proto(&grpc.proto_path, &generated.text)?;
    let pool = compile_proto_pool(&grpc.proto_path).map_err(XJError::internal)?;
    Ok((generated, pool))
}

fn compile_proto_pool(proto_path: &std::path::Path) -> Result<DescriptorPool, String> {
    let file_name = proto_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("xjserver.proto");

    let include_dir = proto_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."));

    let file_descriptor_set = protox::compile([file_name], [include_dir]).map_err(|err| {
        format!(
            "Failed to compile generated proto {}: {err}",
            proto_path.display()
        )
    })?;

    DescriptorPool::from_file_descriptor_set(file_descriptor_set)
        .map_err(|err| format!("Failed to build descriptor pool: {err}"))
}

#[derive(Clone)]
struct GrpcRouterService {
    state: GrpcHandlerState,
    max_recv: usize,
    max_send: usize,
}

impl Service<http::Request<Incoming>> for GrpcRouterService {
    type Response = http::Response<BoxBody>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Incoming>) -> Self::Future {
        let state = self.state.clone();
        let max_recv = self.max_recv;
        let max_send = self.max_send;
        Box::pin(async move { Ok(dispatch_grpc_request(state, req, max_recv, max_send).await) })
    }
}

async fn dispatch_grpc_request(
    state: GrpcHandlerState,
    req: http::Request<Incoming>,
    max_recv: usize,
    max_send: usize,
) -> http::Response<BoxBody> {
    let path = req.uri().path().to_string();
    let (parts, body) = req.into_parts();

    let Some((bucket, route_name)) = parse_grpc_path(&path, &state.package_name) else {
        return status_to_http(Status::not_found(format!("Unknown method: {path}")));
    };

    let request_desc = match request_descriptor(&state.pool, &state.package_name, &route_name) {
        Ok(d) => d,
        Err(status) => return status_to_http(status),
    };

    let body_bytes = match collect_body(body, max_recv).await {
        Ok(b) => b,
        Err(status) => return status_to_http(status),
    };

    let proto_bytes = match decode_grpc_frame(&body_bytes) {
        Ok(b) => b,
        Err(status) => return status_to_http(status),
    };

    let mut message = DynamicMessage::new(request_desc);
    if let Err(err) = message.merge(proto_bytes.as_ref()) {
        return status_to_http(Status::invalid_argument(format!(
            "Failed to decode request: {err}"
        )));
    }

    let mut tonic_req = tonic::Request::new(message);
    *tonic_req.metadata_mut() = headers_to_metadata(&parts.headers);

    match handle_grpc_call(&state, bucket, &route_name, tonic_req).await {
        Ok(response) => {
            let (metadata, message, _extensions) = response.into_parts();
            match encode_success_response(message, &metadata, max_send) {
                Ok(resp) => resp,
                Err(status) => status_to_http(status),
            }
        }
        Err(status) => status_to_http(status),
    }
}

fn parse_grpc_path(path: &str, package_name: &str) -> Option<(RouteBucket, String)> {
    let path = path.trim_start_matches('/');
    let (service_full, method) = path.split_once('/')?;
    let prefix = format!("{package_name}.");
    let service = service_full.strip_prefix(&prefix)?;
    let bucket = RouteBucket::from_grpc_service_name(service)?;
    Some((bucket, method.to_string()))
}

fn headers_to_metadata(headers: &http::HeaderMap) -> tonic::metadata::MetadataMap {
    let mut map = tonic::metadata::MetadataMap::new();
    for (key, value) in headers.iter() {
        if let Ok(val) = value.to_str() {
            if let (Ok(k), Ok(v)) = (
                key.as_str()
                    .parse::<tonic::metadata::MetadataKey<tonic::metadata::Ascii>>(),
                val.parse::<tonic::metadata::MetadataValue<tonic::metadata::Ascii>>(),
            ) {
                map.insert(k, v);
            }
        }
    }
    map
}

async fn collect_body(body: Incoming, max_recv: usize) -> Result<Bytes, Status> {
    let collected = body
        .collect()
        .await
        .map_err(|err| Status::internal(format!("Failed to read body: {err}")))?;
    let bytes = collected.to_bytes();
    if bytes.len() > max_recv {
        return Err(Status::resource_exhausted("message too large"));
    }
    Ok(bytes)
}

fn decode_grpc_frame(bytes: &Bytes) -> Result<Bytes, Status> {
    if bytes.len() < 5 {
        return Err(Status::invalid_argument("Invalid gRPC frame"));
    }
    let compressed = bytes[0];
    if compressed != 0 {
        return Err(Status::unimplemented(
            "Compressed gRPC messages are not supported",
        ));
    }
    let len = u32::from_be_bytes([bytes[1], bytes[2], bytes[3], bytes[4]]) as usize;
    if bytes.len() < 5 + len {
        return Err(Status::invalid_argument("Incomplete gRPC frame"));
    }
    Ok(bytes.slice(5..5 + len))
}

fn encode_grpc_frame(proto_bytes: &[u8], max_send: usize) -> Result<Bytes, Status> {
    if proto_bytes.len() > max_send {
        return Err(Status::resource_exhausted("message too large"));
    }
    let mut buf = BytesMut::with_capacity(5 + proto_bytes.len());
    buf.put_u8(0);
    buf.put_u32(proto_bytes.len() as u32);
    buf.extend_from_slice(proto_bytes);
    Ok(buf.freeze())
}

fn encode_success_response(
    message: DynamicMessage,
    trailing: &tonic::metadata::MetadataMap,
    max_send: usize,
) -> Result<http::Response<BoxBody>, Status> {
    let proto_bytes = message.encode_to_vec();
    let frame = encode_grpc_frame(&proto_bytes, max_send)?;

    // gRPC over HTTP/2 requires grpc-status (and trailing metadata) in
    // trailers after the message body — not in initial response headers.
    let mut trailers = http::HeaderMap::new();
    trailers.insert(
        http::HeaderName::from_static("grpc-status"),
        http::HeaderValue::from_static("0"),
    );
    for key_and_value in trailing.iter() {
        if let tonic::metadata::KeyAndValueRef::Ascii(key, value) = key_and_value {
            if let Ok(val) = value.to_str() {
                if let (Ok(name), Ok(header_val)) = (
                    http::HeaderName::try_from(key.as_str()),
                    http::HeaderValue::try_from(val),
                ) {
                    trailers.insert(name, header_val);
                }
            }
        }
    }

    let body = Full::new(frame).with_trailers(async move { Some(Ok(trailers)) });

    http::Response::builder()
        .status(200)
        .header("content-type", "application/grpc")
        .body(tonic::body::boxed(body))
        .map_err(|err| Status::internal(format!("Failed to build response: {err}")))
}

fn status_to_http(status: Status) -> http::Response<BoxBody> {
    status.into_http()
}
