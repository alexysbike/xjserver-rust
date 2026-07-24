//! gRPC transport: proto generator + dynamic server.

mod codec;
mod error;
mod generate;
mod handler;
mod proto;
mod server;
mod types;
mod write_proto;

pub use generate::{GenerateProtoOptions, generate_proto_from_registry};
pub use server::{prepare_grpc, start_grpc_server};
pub use types::{DEFAULT_PROTO_PACKAGE, GeneratedProto, RouteProtoShape};
pub use write_proto::write_generated_proto;
