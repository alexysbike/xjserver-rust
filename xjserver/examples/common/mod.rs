//! Shared constants and state for `xjserver` examples.

pub const DEV_JWT_SECRET: &str = "xjserver-dev-secret";
pub const DEMO_EMAIL: &str = "ada@gmail.com";
pub const DEMO_PASSWORD: &str = "secret";

#[derive(Clone)]
pub struct AppState {
    pub service: String,
}

impl AppState {
    pub fn demo(service: &str) -> Self {
        Self {
            service: service.into(),
        }
    }
}
