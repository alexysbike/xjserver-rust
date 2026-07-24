use std::collections::HashMap;

use chrono::Utc;
use serde::Serialize;
use serde_json::Value;

use crate::config::XJConfig;
use crate::registry::{ErasedRoute, RouteBucket, RouteRegistry};

pub const XJ_MANIFEST_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteClassInfo {
    pub name: String,
    pub hierarchy: Vec<String>,
    pub base_route: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteTypeSchema {
    #[serde(rename = "jsonSchema")]
    pub json_schema: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteEntry {
    pub name: String,
    pub path: String,
    pub class: RouteClassInfo,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<RouteTypeSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<RouteTypeSchema>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteClassCatalogEntry {
    pub name: String,
    pub extends: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct XJManifest {
    pub manifest_version: String,
    pub generated_at: String,
    pub service: ManifestService,
    pub protocol: ManifestProtocol,
    pub route_classes: Vec<RouteClassCatalogEntry>,
    pub routes: ManifestRoutes,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestService {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub http_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grpc_port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introspection_port: Option<u16>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestProtocol {
    #[serde(rename = "type")]
    pub protocol_type: String,
    pub transports: Vec<String>,
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grpc: Option<ManifestGrpc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introspection: Option<ManifestIntrospection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ManifestClient>,
    pub namespaces: ManifestNamespaces,
    pub headers: ManifestHeaders,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestGrpc {
    pub package: String,
    pub proto_path: String,
    pub port: u16,
    pub keep_case: bool,
    pub services: ManifestGrpcServices,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct ManifestGrpcServices {
    pub xj: Vec<String>,
    pub login: Vec<String>,
    pub session: Vec<String>,
    pub logout: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestIntrospection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    pub endpoints: ManifestIntrospectionEndpoints,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestIntrospectionEndpoints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub health: Option<ManifestEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<ManifestEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explorer: Option<ManifestEndpoint>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestEndpoint {
    pub method: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestClient {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_base_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestNamespaces {
    pub xj: ManifestNamespaceXj,
    pub login: ManifestNamespaceLogin,
    pub session: ManifestNamespaceSession,
    pub logout: ManifestNamespaceLogout,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestNamespaceXj {
    pub method: String,
    pub path_pattern: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestNamespaceLogin {
    pub method: String,
    pub path_pattern: String,
    pub issues_token: bool,
    pub token_header: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestNamespaceSession {
    pub method: String,
    pub path_pattern: String,
    pub requires_auth: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestNamespaceLogout {
    pub method: String,
    pub path_pattern: String,
    pub requires_auth: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestHeaders {
    pub auth: String,
    pub auth_format: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestRoutes {
    pub xj: Vec<RouteEntry>,
    pub login: Vec<RouteEntry>,
    pub logout: Vec<RouteEntry>,
    pub session: Vec<RouteEntry>,
}

/// Build the runtime manifest (paridad Node `buildManifest`).
pub fn build_manifest(
    registry: &RouteRegistry,
    config: &XJConfig,
    http_port: Option<u16>,
) -> XJManifest {
    build_manifest_with_grpc(registry, config, http_port, None)
}

/// Build manifest including optional gRPC section from a generated proto.
pub fn build_manifest_with_grpc(
    registry: &RouteRegistry,
    config: &XJConfig,
    http_port: Option<u16>,
    generated_proto: Option<&crate::grpc::GeneratedProto>,
) -> XJManifest {
    let token_header = config.token_header.clone();
    let token_prefix = config.token_prefix.clone();

    let routes = ManifestRoutes {
        xj: bucket_entries(registry, RouteBucket::Xj),
        login: bucket_entries(registry, RouteBucket::Login),
        logout: bucket_entries(registry, RouteBucket::Logout),
        session: bucket_entries(registry, RouteBucket::Session),
    };

    let all_entries = routes
        .xj
        .iter()
        .chain(routes.login.iter())
        .chain(routes.logout.iter())
        .chain(routes.session.iter())
        .cloned()
        .collect::<Vec<_>>();

    let introspection = build_introspection_section(config);
    let client = build_client_section(config);

    let mut transports = Vec::new();
    if config.http.port.is_some() || http_port.is_some() {
        transports.push("http".into());
    }
    // Always advertise http if we have routes served over HTTP adapter historically;
    // keep prior behaviour when only HTTP was used.
    if transports.is_empty() {
        transports.push("http".into());
    }

    let grpc_port = config.grpc.as_ref().map(|g| g.port);
    let grpc_section = match (config.grpc.as_ref(), generated_proto) {
        (Some(grpc), Some(generated)) => {
            if !transports.iter().any(|t| t == "grpc") {
                transports.push("grpc".into());
            }
            Some(ManifestGrpc {
                package: generated.package_name.clone(),
                proto_path: grpc.proto_path.display().to_string(),
                port: grpc.port,
                keep_case: grpc.keep_case,
                services: ManifestGrpcServices {
                    xj: generated
                        .services
                        .get(&RouteBucket::Xj)
                        .cloned()
                        .unwrap_or_default(),
                    login: generated
                        .services
                        .get(&RouteBucket::Login)
                        .cloned()
                        .unwrap_or_default(),
                    session: generated
                        .services
                        .get(&RouteBucket::Session)
                        .cloned()
                        .unwrap_or_default(),
                    logout: generated
                        .services
                        .get(&RouteBucket::Logout)
                        .cloned()
                        .unwrap_or_default(),
                },
            })
        }
        (Some(grpc), None) => {
            if !transports.iter().any(|t| t == "grpc") {
                transports.push("grpc".into());
            }
            Some(ManifestGrpc {
                package: grpc.package.clone(),
                proto_path: grpc.proto_path.display().to_string(),
                port: grpc.port,
                keep_case: grpc.keep_case,
                services: ManifestGrpcServices {
                    xj: Vec::new(),
                    login: Vec::new(),
                    session: Vec::new(),
                    logout: Vec::new(),
                },
            })
        }
        _ => None,
    };

    XJManifest {
        manifest_version: XJ_MANIFEST_VERSION.to_string(),
        generated_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        service: ManifestService {
            name: config.service_name.clone(),
            http_port: http_port.or(config.http.port),
            grpc_port,
            introspection_port: config.introspection.as_ref().and_then(|i| i.port),
        },
        protocol: ManifestProtocol {
            protocol_type: "xj-rpc".into(),
            transports,
            content_type: "application/json".into(),
            grpc: grpc_section,
            introspection,
            client,
            namespaces: ManifestNamespaces {
                xj: ManifestNamespaceXj {
                    method: "POST".into(),
                    path_pattern: "/xj/{name}".into(),
                },
                login: ManifestNamespaceLogin {
                    method: "POST".into(),
                    path_pattern: "/login/{name}".into(),
                    issues_token: true,
                    token_header: token_header.clone(),
                },
                session: ManifestNamespaceSession {
                    method: "POST".into(),
                    path_pattern: "/session/{name}".into(),
                    requires_auth: true,
                },
                logout: ManifestNamespaceLogout {
                    method: "POST".into(),
                    path_pattern: "/logout/{name}".into(),
                    requires_auth: true,
                },
            },
            headers: ManifestHeaders {
                auth: token_header,
                auth_format: format!("{token_prefix} {{token}}"),
            },
        },
        route_classes: build_route_classes(&all_entries),
        routes,
    }
}

fn bucket_entries(registry: &RouteRegistry, bucket: RouteBucket) -> Vec<RouteEntry> {
    let mut entries: Vec<RouteEntry> = registry
        .routes_in_bucket(bucket)
        .into_iter()
        .map(|route| to_route_entry(route.as_ref(), bucket))
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

fn to_route_entry(route: &dyn ErasedRoute, bucket: RouteBucket) -> RouteEntry {
    let name = route.name();
    RouteEntry {
        name: name.to_string(),
        path: format!("/{}/{}", bucket.as_str(), name),
        class: route.route_class_info(),
        input: route
            .input_json_schema()
            .map(|json_schema| RouteTypeSchema { json_schema }),
        output: route
            .output_json_schema()
            .map(|json_schema| RouteTypeSchema { json_schema }),
    }
}

pub fn route_class_info_from_type_name(type_name: &str) -> RouteClassInfo {
    let short = type_name.rsplit("::").next().unwrap_or(type_name);
    RouteClassInfo {
        name: short.to_string(),
        hierarchy: vec![short.to_string(), "XJRoute".to_string()],
        base_route: "XJRoute".to_string(),
    }
}

fn build_route_classes(entries: &[RouteEntry]) -> Vec<RouteClassCatalogEntry> {
    let mut extends_map: HashMap<String, Option<String>> = HashMap::new();

    for entry in entries {
        let hierarchy = &entry.class.hierarchy;
        for i in 1..hierarchy.len() {
            let name = hierarchy[i].clone();
            let parent = if i + 1 < hierarchy.len() {
                Some(hierarchy[i + 1].clone())
            } else {
                None
            };

            match extends_map.get(&name) {
                None => {
                    extends_map.insert(name, parent);
                }
                Some(existing) if *existing != parent => {
                    eprintln!(
                        "[XJServer] routeClasses conflict: '{name}' extends '{existing:?}' and '{parent:?}'"
                    );
                }
                _ => {}
            }
        }
    }

    let mut classes: Vec<RouteClassCatalogEntry> = extends_map
        .into_iter()
        .map(|(name, extends)| RouteClassCatalogEntry { name, extends })
        .collect();
    classes.sort_by(|a, b| a.name.cmp(&b.name));
    classes
}

fn build_introspection_section(config: &XJConfig) -> Option<ManifestIntrospection> {
    let introspection = config.introspection.as_ref()?;
    let health = introspection.health.as_ref();
    let manifest = introspection.manifest.as_ref();
    let explorer = introspection.explorer.as_ref();

    if health.is_none() && manifest.is_none() {
        return None;
    }

    let mut endpoints = ManifestIntrospectionEndpoints {
        health: None,
        manifest: None,
        explorer: None,
    };

    if let Some(h) = health {
        endpoints.health = Some(ManifestEndpoint {
            method: "GET".into(),
            path: h.path.clone(),
        });
    }

    if let Some(m) = manifest {
        endpoints.manifest = Some(ManifestEndpoint {
            method: "GET".into(),
            path: m.path.clone(),
        });
    }

    if let Some(e) = explorer {
        endpoints.explorer = Some(ManifestEndpoint {
            method: "GET".into(),
            path: e.path.clone(),
        });
    }

    Some(ManifestIntrospection {
        port: introspection.port,
        endpoints,
    })
}

fn build_client_section(config: &XJConfig) -> Option<ManifestClient> {
    if let Some(client) = &config.manifest_client {
        if client.api_base_url.is_some() {
            return Some(ManifestClient {
                api_base_url: client.api_base_url.clone(),
            });
        }
    }

    if config
        .introspection
        .as_ref()
        .is_some_and(|i| i.port.is_none())
    {
        return Some(ManifestClient {
            api_base_url: Some(String::new()),
        });
    }

    None
}
