# xjserver Roadmap

## Improvement points

### 1. Rate limiter is in-memory only

Uses a local `governor` (`RateLimiter::keyed`). This is a real problem given the architecture: if xjserver microservices scale horizontally (as with the WebSocket service), each instance keeps its own counter — an attacker rotating across instances can evade the limit, and legitimate users can be blocked inconsistently.

Since Redis is already in the ecosystem, abstract the rate limiter behind a trait (`RateLimitStore`) with:

- a default in-memory implementation
- a Redis-backed implementation (e.g. reuse a sliding-window pattern similar to what may already be planned for notifications)

### 2. Session is resolved twice per request

`session_middleware.resolve()` is called in both `xj_rate_limit` (middleware) and again in `dispatch_bucket` — the JWT is decoded twice per request. Resolve it once and pass the `Session` via Axum request `Extensions` into the handler.

### 3. `XJError` does not implement `std::error::Error`

Today it is an enum of plain `String`s; that loses the `source()` chain and forces manual mapping everywhere when using `?` with errors from other libraries. Add `thiserror` (already on the suggested crates list) and keep the original error as `source` for debugging/observability, without breaking the current JSON serialization.

### 4. Missing observability (tracing / metrics)

There is no `tracing` integration (spans per request/route) and no Prometheus-style metrics (latency, error rate per route). For a microservices ecosystem this is nearly mandatory — add a `tower-http::TraceLayer` or custom instrumentation in the dispatcher, plus a `/metrics` endpoint.

### 5. gRPC proto is regenerated dynamically on every startup

Today the `.proto` is regenerated on every `serve_grpc()`. Convenient, but a contract risk: if types change, field numbers can shift across deployments and break compatibility with already-generated clients. For v2, freeze the generated proto (version it, commit it) and regenerate only explicitly — not on every boot.

### 6. JWT only supports HS256

For a gateway + JWT validation via WASM scenario, JWKS/RS256 is likely preferable (public key on the gateway, private key on the issuer) instead of a shared symmetric secret. A solid addition: support for `Algorithm::RS256` plus key rotation (`kid`).

---

## Possible features

### Core platform

#### 7. Native streaming / WebSocket support as an additional bucket

Given that real-time notifications are being modeled, a fifth `ws` bucket (or SSE support inside xjserver, instead of a fully separate microservice) would simplify the architecture — though keeping it separate, as already planned, is also valid.

#### 8. Typed client generation

Since the manifest already includes a JSON Schema per route, generating a typed client (TS, Rust, Python, or Dart) from it would be a big productivity win for consuming xjserver from Topcoat or Node apps.

#### 9. Test harness

A utility to invoke an `XJRoute` directly (build a test `Context`, run `can_execute` + `execute`) without starting the HTTP server — would reduce friction for unit-testing routes.

#### 10. Body limit / rate limit per route

Today both are global; some routes (upload, bulk notifications) may need different limits.

#### 11. Fine-grained authorization integration (OpenFGA / Keto)

Today `can_execute` is a simple boolean. Add an injectable `Authorizer` trait (similar to `SessionMiddleware`) that receives `(session, tenant, resource, action)` and delegates to OpenFGA — keeping `can_execute` for simple cases and this new hook for relational authorization.

#### 12. Server-level lifecycle hooks (`before_dispatch` / `after_dispatch`)

A `Vec<Arc<dyn DispatchHook>>` that runs before/after each request, for auditing, structured logging, or injecting shared context (`trace-id`, `tenant`) without each route repeating it.

#### 13. Idempotency keys

Optional support for an `Idempotency-Key` header with a short-lived cache (in-memory or Redis) that returns the same response when the same request is retried — very useful for routes like creating an order in sales/purchasing.

#### 14. Per-route timeouts

Today if an `execute` hangs, there is no cutoff; `#[xj_route(timeout_ms = 3000)]` or global config with `tokio::time::timeout` would prevent hung requests from consuming workers.

#### 15. Retries / circuit breaker for outbound calls

If xjserver routes call other microservices (gRPC), a `Deps<T>`-style extractor that wraps clients with `tower::retry` + circuit breaker would standardize the pattern across the ecosystem.

#### 16. Config loading from file/env with strong validation

Today `XJConfig` is built 100% in code; `XJConfig::from_env()` or `figment` support (already on the list) would allow changing `jwt_secret`, ports, limits, without recompiling — important for the same binary across dev/staging/prod.

#### 17. Graceful shutdown

Today `run()` blocks without handling signals; add `tokio::signal::ctrl_c()` + in-flight request draining (give time to finish before closing the listener) — critical in k8s (`SIGTERM`).

#### 18. Refresh tokens / session rotation

Today only an access JWT is issued; add a refresh flow (with revocation) to avoid fixed 10h sessions with no way to invalidate early.

#### 19. Token revocation (blocklist)

A hook to check a store (Redis) of revoked tokens before accepting a valid JWT — needed for real logout or user bans.

#### 20. Route versioning

Convention `#[xj_route(version = "v2")]` or namespace `xj/v2/...`, to evolve a route without breaking old clients.

#### 21. Deprecation warnings in the manifest

Mark routes as deprecated with a message, visible in the explorer and in response headers (`Sunset` / `Deprecation`) — useful in an ecosystem with many internal consumers.

#### 22. Contract testing

Generate manifest snapshots and test in CI that there are no unintentional breaking changes (removed field, changed type) between service versions.

#### 23. xjserver CLI (`cargo xjserver routes`, `cargo xjserver proto`)

List registered routes, generate the `.proto` without starting the server, validate duplicates — useful in CI before deploy.

#### 24. Configurable structured logging (JSON vs text)

Separate from tracing, for easy integration with the rest of the stack (ELK, Loki, etc.).

### HTTP & integration

#### 25. Payload compression

Add `gzip` / `br` via `tower-http::CompressionLayer` — trivial to add and `tower-http` is already in the stack.

#### 26. OpenAPI export alongside the native manifest

JSON Schema is already generated per route; from that, emit an `openapi.json` almost for free to integrate with standard tools (Postman, auto-generated clients in other languages) without being tied only to the XJ format.

#### 27. Additional namespaces or custom buckets

Beyond `xj`, support other namespaces to better organize routes.

#### 28. gRPC proto discovery

Add proto discovery (or similar) for gRPC.

### Testing & quality

#### 29. Chaos testing hooks

Inject artificial latency/errors on specific routes via config (non-prod only) to test how the rest of the ecosystem reacts.

#### 30. Per-route example fixtures

Extend contract snapshot testing with optional `example_input()` / `example_output()` on the trait — feeds both tests and the explorer with real sample data.

#### 31. Auto-fuzzing in CI

Using generated JSON Schemas, produce random/edge-case inputs (empty strings, negative numbers, unusual unicode) and run them against each route in a test environment — catch panics before production without writing a manual fuzz test per route.

#### 32. Breaking change detection against federated manifest

In CI, compare the new manifest against what other real services already consume (not just a local snapshot) and warn before breaking `directory`, `ventas`, etc.

#### 33. Route "merge conflicts"

If two teams independently register a route with the same name but incompatible schemas in two services that will later be federated, detect the semantic conflict before deploy — not at runtime.

### Developer experience

#### 34. Terminal explorer with `ratatui`

An `xj` explorer in the terminal, built with Rust `ratatui`.

#### 35. Richer manifest documentation

Add more manifest fields — e.g. a description of what each route does.

#### 36. `can_execute` metadata in the manifest

Expose each route's `can_execute` in the manifest when set, so the explorer can show it. Optionally add `description` on `can_execute` checks for human-readable explorer text.

#### 37. Route composition (server-side pipelines)

A declarative way to say "this route is actually A → B → C" (e.g. `createOrderAndNotify`) without the handler manually orchestrating other modules — something like `Pipeline<In, Out>` that chains existing `XJRoute`s.

#### 38. Universal dry-run mode

Any executable route with `?dryRun=true` runs `can_execute` + validation but not the real `execute` (or a side-effect-free variant if the trait supports it) — useful for frontends validating before a costly action.

#### 39. Real request recording and replay

Opt-in recording (with sanitized data) to reproduce a bug locally with the exact request that triggered it. Debug middleware saves `(input, session, result)` to structured logs; `xjserver replay <id>` re-executes against the local binary.

#### 40. "Record session" in the explorer

Export what you tested in the explorer as an integration test (`.rs` or `.http` file) with one click — closing the loop between manual exploration and permanent tests.

#### 41. Integrated load simulator

`cargo xjserver bench <routeName>` generates synthetic load locally using in-memory `execute` (no network) to measure per-route throughput before deploy, without standing up a real server.

#### 42. Explain why `can_execute` failed

Today it returns a plain `bool` (fail-closed, no reason). An optional variant returning `Result<(), DenialReason>` so the `Forbidden` response explains why (without leaking sensitive info) instead of a generic message — better error UX without sacrificing security.

### Manifest & ecosystem

#### 43. Federated manifest

An xjserver can declare "I know the identity manifest at this URL" and the explorer/docs show the full ecosystem map (all microservices and routes) in one place — a central explorer that aggregates manifests.

#### 44. Manifest signing

Cryptographically sign the manifest at generation time so clients (or a gateway) can verify it was not tampered with in transit — relevant when the manifest drives auto-generated clients in a pipeline.

#### 45. Git-style versioned manifest

Instead of only "the current manifest", each schema change generates an internal commit with a readable diff ("`email` went from required to optional in `createUser`"), navigable as history — a git log for API contracts, without using Git itself.

#### 46. Conversational manifest

An endpoint where the service answers questions about itself ("which routes modify stock?") using the manifest as context — a mini agent per microservice that can "interview" itself.

#### 47. Auto-generated natural language descriptions

From route name + in/out JSON Schemas + possible error types, generate (LLM, offline, at build time) readable copy like "this route creates an order, requires an authenticated session, fails if stock is insufficient" — lives in the manifest, regenerates each build, never goes stale.

### Security

#### 48. Honeypot routes

Deliberately registered phantom routes with tempting names (`admin`, `debug`, `internal`) that should never be called legitimately — hits are logged as probable recon/attack attempts, without exposing a real route.

#### 49. Canary tokens in JWT

Occasionally issue trap tokens (for test/honeypot accounts); if they appear used from an unexpected place, that signals a secret leak.

### Observability & resilience

#### 50. Contract "immune system"

If a client microservice starts receiving far more `ValidationBadRequest` than usual for a specific route, the server self-reports ("this route is broken for external consumers") before someone opens a ticket.

#### 51. Execution history attached to resources

Each `execute` that modifies an entity (order, user) appends an event to a lightweight append-only log tied to that entity — not full event sourcing, but an automatic per-entity changelog ("who touched this order and when?").

#### 52. Route "emotional health"

Each route gets a stress score from latency, error rate, and rate-limit hits; the explorer renders a mood traffic light (green calm, yellow nervous, red panicking) — a microservices tamagotchi instead of a flat metrics table.

### Experimental / playful

#### 53. Playground with AI-generated synthetic data

Using existing JSON Schemas, auto-generate realistic example inputs (not empty placeholders) to test routes from the explorer without hand-crafting payloads.

#### 54. Explorer as a city map

Each microservice is a neighborhood, each route a building, inter-service calls are streets with live traffic — a SimCity view of the real architecture, more intuitive for onboarding than a README.

#### 55. Sound as monitoring

Sonify system traffic (serious error = dissonant tone, all clear = calm ambience) as background audio in the office/stream — feel when something is wrong without staring at a dashboard.

#### 56. Service autobiography on deploy

Each new deployment generates a first-person narrative paragraph ("today I learned to validate emails better") from the code diff — a life diary of the microservice.

#### 57. Replace axum for hyper

Replace the http implementation of axum for hyper.

#### 58. JSON5 for xj explorer.

In the Explorer (introspection.rs) — the "try it" acepts JSON5 in the textarea, parse it on the browser, and send it like JSON to the backend.

#### 59. Multipart http routes

Add support for form multipart for uploads