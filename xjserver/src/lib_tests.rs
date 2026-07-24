#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;
    use tower::ServiceExt;

    use crate::error::RouteValidationIssue;
    use crate::http::introspection::render_explorer_page;
    use crate::manifest::{build_manifest, XJ_MANIFEST_VERSION};
    use crate::registry::RouteRegistry;
    use crate::route::XJRoute;
    use crate::validation::validate_json;
    use crate::{
        Context, GenerateProtoOptions, GrpcConfig, IntrospectionConfig, XJConfig, XJError,
        XJServer, generate_proto_from_registry, prepare_grpc,
    };

    #[derive(Debug, Deserialize, JsonSchema)]
    struct GreetIn {
        name: String,
    }

    struct GreetRoute;

    #[async_trait::async_trait]
    impl XJRoute for GreetRoute {
        type In = GreetIn;
        type Out = String;

        fn name(&self) -> &'static str {
            "greet"
        }

        async fn execute(&self, ctx: &mut Context<GreetIn>) -> Result<String, XJError> {
            Ok(format!("hi {}", ctx.data().name))
        }
    }

    #[test]
    fn validation_rejects_missing_required_field() {
        let err = validate_json::<GreetIn>(&json!({}), "greet").unwrap_err();
        match err {
            XJError::ValidationBadRequest { issues, .. } => {
                assert!(!issues.is_empty());
            }
            other => panic!("expected ValidationBadRequest, got {other:?}"),
        }
    }

    #[test]
    fn validation_accepts_valid_input() {
        validate_json::<GreetIn>(&json!({"name": "ada"}), "greet").unwrap();
    }

    #[test]
    fn manifest_has_node_compatible_shape() {
        let mut registry = RouteRegistry::new();
        registry.add_xj(GreetRoute).unwrap();

        let config = XJConfig {
            service_name: Some("test-service".into()),
            introspection: Some(IntrospectionConfig::enabled()),
            ..XJConfig::default()
        };

        let manifest = build_manifest(&registry, &config, Some(3003));

        assert_eq!(manifest.manifest_version, XJ_MANIFEST_VERSION);
        assert_eq!(manifest.service.name.as_deref(), Some("test-service"));
        assert_eq!(manifest.service.http_port, Some(3003));
        assert_eq!(manifest.protocol.transports, vec!["http"]);
        assert_eq!(manifest.routes.xj.len(), 1);
        assert_eq!(manifest.routes.xj[0].name, "greet");
        assert_eq!(manifest.routes.xj[0].path, "/xj/greet");
        assert!(manifest.routes.xj[0].input.is_some());
        assert!(manifest.routes.xj[0].output.is_some());
        assert_eq!(
            manifest.protocol.headers.auth_format,
            "Bearer {token}"
        );

        let introspection = manifest.protocol.introspection.as_ref().unwrap();
        assert_eq!(
            introspection.endpoints.health.as_ref().unwrap().path,
            "/health"
        );
        assert_eq!(
            introspection.endpoints.manifest.as_ref().unwrap().path,
            "/__xj/manifest"
        );
        assert_eq!(
            introspection.endpoints.explorer.as_ref().unwrap().path,
            "/__xj/docs"
        );
    }

    #[test]
    fn generate_proto_from_registry_emits_services() {
        let mut registry = RouteRegistry::new();
        registry.add_xj(GreetRoute).unwrap();

        let generated =
            generate_proto_from_registry(&registry, GenerateProtoOptions::default()).unwrap();

        assert!(generated.text.contains("syntax = \"proto3\";"));
        assert!(generated.text.contains("package xjserver.v1;"));
        assert!(generated.text.contains("service Xj {"));
        assert!(generated.text.contains("rpc greet(greetRequest) returns (greetResponse);"));
        assert!(generated.text.contains("message greetRequest"));
        assert_eq!(generated.shapes.len(), 1);
        assert_eq!(generated.shapes[0].route_name, "greet");
    }

    #[test]
    fn prepare_grpc_writes_and_compiles_proto() {
        let mut registry = RouteRegistry::new();
        registry.add_xj(GreetRoute).unwrap();

        let dir = std::env::temp_dir().join(format!(
            "xjserver-proto-test-{}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let proto_path = dir.join("test.proto");

        let grpc = GrpcConfig::new(50051, &proto_path);
        let (generated, pool) = prepare_grpc(&registry, &grpc).unwrap();

        assert!(proto_path.exists());
        assert!(generated.text.contains("service Xj"));
        assert!(
            pool.get_message_by_name("xjserver.v1.greetRequest")
                .is_some()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn error_json_parity_bad_request() {
        let err = XJError::bad_request("Invalid token data");
        assert_eq!(err.status_code(), 400);
        assert_eq!(err.to_json_body(), json!({"error": "Invalid token data"}));
    }

    #[test]
    fn error_json_parity_validation() {
        let err = XJError::validation_bad_request(
            "Invalid input",
            vec![RouteValidationIssue {
                path: "/name".into(),
                message: "required".into(),
                code: Some("required".into()),
            }],
        );
        assert_eq!(err.status_code(), 400);
        assert_eq!(
            err.to_json_body(),
            json!({
                "error": "Invalid input",
                "issues": [{"path": "/name", "message": "required", "code": "required"}]
            })
        );
    }

    #[test]
    fn error_json_parity_forbidden_not_found_internal_429() {
        assert_eq!(
            XJError::forbidden("nope").to_json_body(),
            json!({"error": "nope"})
        );
        assert_eq!(XJError::forbidden("nope").status_code(), 403);

        assert_eq!(
            XJError::not_found("missing").to_json_body(),
            json!({"error": "missing"})
        );
        assert_eq!(XJError::not_found("missing").status_code(), 404);

        assert_eq!(
            XJError::internal("boom").to_json_body(),
            json!({"error": "boom"})
        );
        assert_eq!(XJError::internal("boom").status_code(), 500);

        assert_eq!(
            XJError::TooManyRequests.to_json_body(),
            json!({"error": "Too many requests"})
        );
        assert_eq!(XJError::TooManyRequests.status_code(), 429);
    }

    #[test]
    fn explorer_page_escapes_attributes() {
        let html = render_explorer_page(
            r#"/__xj/explorer/xj-explorer.js""#,
            r#"/__xj/manifest"><script>"#,
        );
        assert!(html.contains("data-manifest-url=\"/__xj/manifest&quot;&gt;&lt;script&gt;\""));
        assert!(html.contains("src=\"/__xj/explorer/xj-explorer.js&quot;\""));
        assert!(html.contains("id=\"xj-explorer-root\""));
    }

    #[tokio::test]
    async fn security_headers_on_health() {
        let config = XJConfig {
            introspection: Some(IntrospectionConfig::enabled()),
            ..XJConfig::default()
        };
        let server = XJServer::builder()
            .config(config)
            .route(GreetRoute)
            .state(())
            .expect("build");

        let response = server
            .into_router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/health")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let headers = response.headers();
        assert_eq!(headers.get("x-content-type-options").unwrap(), "nosniff");
        assert_eq!(headers.get("x-frame-options").unwrap(), "DENY");
        assert_eq!(headers.get("referrer-policy").unwrap(), "no-referrer");
        assert_eq!(headers.get("x-dns-prefetch-control").unwrap(), "off");
    }

    #[tokio::test]
    async fn builder_config_shortcuts_appear_in_manifest() {
        let server = XJServer::builder()
            .service_name("via-shortcuts")
            .jwt_secret("dev")
            .http_port(3006)
            .introspection(IntrospectionConfig::enabled())
            .grpc(50052, "target/xjserver-grpc-demo.proto")
            .route(GreetRoute)
            .state(())
            .expect("build");

        let response = server
            .into_router()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/__xj/manifest")
                    .body(axum::body::Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), 200);
        let body: serde_json::Value = serde_json::from_slice(
            &response
                .into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes(),
        )
        .unwrap();
        assert_eq!(body["service"]["name"], "via-shortcuts");
        assert_eq!(body["service"]["grpcPort"], 50052);
        assert!(
            body["protocol"]["transports"]
                .as_array()
                .unwrap()
                .iter()
                .any(|t| t == "grpc")
        );
    }

    #[tokio::test]
    async fn explorer_docs_and_asset_served() {
        let config = XJConfig {
            introspection: Some(IntrospectionConfig::enabled()),
            ..XJConfig::default()
        };
        let server = XJServer::builder()
            .config(config)
            .route(GreetRoute)
            .state(())
            .expect("build");

        let mut router = server.into_router();

        let docs = ServiceExt::<axum::http::Request<axum::body::Body>>::oneshot(
            &mut router,
            axum::http::Request::builder()
                .uri("/__xj/docs")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(docs.status(), 200);
        let docs_body = String::from_utf8(
            docs.into_body()
                .collect()
                .await
                .unwrap()
                .to_bytes()
                .to_vec(),
        )
        .unwrap();
        assert!(docs_body.contains("xj-explorer-root"));
        assert!(docs_body.contains("/__xj/manifest"));
        assert!(docs_body.contains("/__xj/explorer/xj-explorer.js"));

        let asset = ServiceExt::<axum::http::Request<axum::body::Body>>::oneshot(
            &mut router,
            axum::http::Request::builder()
                .uri("/__xj/explorer/xj-explorer.js")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(asset.status(), 200);
        let ct = asset
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(ct.starts_with("application/javascript"));
        let bytes = asset.into_body().collect().await.unwrap().to_bytes();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn erase_and_add_erased_roundtrip() {
        use crate::registry::erase;
        use crate::{RouteBucket, RouteRegistry};

        let mut registry = RouteRegistry::new();
        registry
            .add_erased(RouteBucket::Xj, erase(GreetRoute))
            .unwrap();
        assert!(registry.get_xj("greet").is_some());

        let err = registry
            .add_erased(RouteBucket::Login, erase(GreetRoute))
            .unwrap_err();
        assert!(
            err.message()
                .contains("Duplicate route registration"),
            "{}",
            err.message()
        );
    }

    #[cfg(feature = "discover")]
    mod discover_tests {
        use super::*;
        use crate::{RouteBucket, RouteRegistration, erase};

        #[crate::xj_register(xj)]
        struct DiscoveredPing;

        #[async_trait::async_trait]
        impl XJRoute for DiscoveredPing {
            type In = crate::Empty;
            type Out = String;

            fn name(&self) -> &'static str {
                "__phase8_discovered_ping"
            }

            async fn execute(
                &self,
                _ctx: &mut Context<crate::Empty>,
            ) -> Result<String, XJError> {
                Ok("pong".into())
            }
        }

        #[tokio::test]
        async fn builder_loads_xj_register_route() {
            let server = XJServer::builder()
                .introspection(IntrospectionConfig::enabled())
                .state(())
                .expect("discover");

            let response = server
                .into_router()
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/__xj/manifest")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(response.status(), 200);
            let body: serde_json::Value = serde_json::from_slice(
                &response
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes(),
            )
            .unwrap();

            let names: Vec<&str> = body["routes"]["xj"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|r| r["name"].as_str())
                .collect();
            assert!(
                names.contains(&"__phase8_discovered_ping"),
                "expected discovered route in manifest, got {names:?}"
            );
        }

        #[tokio::test]
        async fn skip_discover_ignores_inventory() {
            let server = XJServer::builder()
                .skip_discover()
                .introspection(IntrospectionConfig::enabled())
                .state(())
                .expect("build");

            let response = server
                .into_router()
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/__xj/manifest")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            let body: serde_json::Value = serde_json::from_slice(
                &response
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes(),
            )
            .unwrap();

            let names: Vec<&str> = body["routes"]["xj"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|r| r["name"].as_str())
                .collect();
            assert!(
                !names.contains(&"__phase8_discovered_ping"),
                "skip_discover should not load inventory routes"
            );
        }

        #[tokio::test]
        async fn builder_without_discover_ignores_inventory() {
            let server = XJServer::builder_without_discover()
                .introspection(IntrospectionConfig::enabled())
                .route(GreetRoute)
                .state(())
                .expect("build");

            let response = server
                .into_router()
                .oneshot(
                    axum::http::Request::builder()
                        .uri("/__xj/manifest")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            let body: serde_json::Value = serde_json::from_slice(
                &response
                    .into_body()
                    .collect()
                    .await
                    .unwrap()
                    .to_bytes(),
            )
            .unwrap();

            let names: Vec<&str> = body["routes"]["xj"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|r| r["name"].as_str())
                .collect();
            assert_eq!(names, vec!["greet"]);
        }

        #[test]
        fn inventory_submit_visible_to_iter() {
            let _ = RouteRegistration {
                bucket: RouteBucket::Xj,
                factory: || erase(GreetRoute),
            };
            let found = inventory::iter::<RouteRegistration>
                .into_iter()
                .any(|r| (r.factory)().name() == "__phase8_discovered_ping");
            assert!(found, "xj_register submit should be in inventory");
        }
    }

    /// Compiles with `--no-default-features`: discover APIs are absent; manual registry works.
    #[cfg(not(feature = "discover"))]
    #[test]
    fn manual_registration_without_discover_feature() {
        use crate::registry::erase;
        use crate::{RouteBucket, RouteRegistry};

        let mut registry = RouteRegistry::new();
        registry
            .add_erased(RouteBucket::Xj, erase(GreetRoute))
            .unwrap();
        assert!(registry.get_xj("greet").is_some());

        let server = XJServer::builder()
            .route(GreetRoute)
            .state(())
            .expect("manual build without discover");
        let _ = server;
    }
}
