use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::XJError;

/// Authenticated (or guest) session for the current request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Session {
    pub name: String,
    pub id: i64,
    /// Full JWT / session payload (includes `name`/`id` and any extras).
    pub claims: Value,
}

impl Session {
    pub fn guest() -> Self {
        let claims = serde_json::json!({
            "name": "guest",
            "id": -1,
        });
        Self {
            name: "guest".to_string(),
            id: -1,
            claims,
        }
    }

    pub fn is_guest(&self) -> bool {
        self.id == -1
    }

    /// Build a session from JWT claims (or login result). Requires `name` (string)
    /// and `id` (number). Missing / invalid fields → `BadRequest("Invalid token data")`.
    pub fn from_claims(claims: Value) -> Result<Self, XJError> {
        let obj = claims.as_object().ok_or_else(|| {
            XJError::bad_request("Invalid token data")
        })?;

        let name = obj
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| XJError::bad_request("Invalid token data"))?
            .to_string();

        let id = obj
            .get("id")
            .and_then(|v| {
                v.as_i64()
                    .or_else(|| v.as_u64().and_then(|n| i64::try_from(n).ok()))
                    .or_else(|| v.as_f64().map(|n| n as i64))
            })
            .ok_or_else(|| XJError::bad_request("Invalid token data"))?;

        Ok(Self {
            name,
            id,
            claims: Value::Object(obj.clone()),
        })
    }

    /// Convenience: build from a map of claims.
    pub fn from_claims_map(map: Map<String, Value>) -> Result<Self, XJError> {
        Self::from_claims(Value::Object(map))
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::guest()
    }
}
