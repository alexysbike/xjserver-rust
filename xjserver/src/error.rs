use serde::Serialize;

#[derive(Debug, Clone)]
pub enum XJError {
    BadRequest { message: String },
    ValidationBadRequest {
        message: String,
        issues: Vec<RouteValidationIssue>,
    },
    Forbidden { message: String },
    NotFound { message: String },
    InternalServerError { message: String },
    TooManyRequests,
}

#[derive(Debug, Clone, Serialize)]
pub struct RouteValidationIssue {
    pub path: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl XJError {
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::BadRequest {
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden {
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::NotFound {
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::InternalServerError {
            message: message.into(),
        }
    }

    pub fn validation_bad_request(
        message: impl Into<String>,
        issues: Vec<RouteValidationIssue>,
    ) -> Self {
        Self::ValidationBadRequest {
            message: message.into(),
            issues,
        }
    }

    pub fn status_code(&self) -> u16 {
        match self {
            Self::BadRequest { .. } | Self::ValidationBadRequest { .. } => 400,
            Self::Forbidden { .. } => 403,
            Self::NotFound { .. } => 404,
            Self::TooManyRequests => 429,
            Self::InternalServerError { .. } => 500,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::BadRequest { message }
            | Self::ValidationBadRequest { message, .. }
            | Self::Forbidden { message }
            | Self::NotFound { message }
            | Self::InternalServerError { message } => message,
            Self::TooManyRequests => "Too many requests",
        }
    }

    pub fn to_json_body(&self) -> serde_json::Value {
        let issues = match self {
            Self::ValidationBadRequest { issues, .. } => Some(issues.as_slice()),
            _ => None,
        };
        serde_json::to_value(ErrorBody {
            error: self.message(),
            issues,
        })
        .unwrap_or_else(|_| serde_json::json!({ "error": "Internal server error" }))
    }
}

#[derive(Serialize)]
struct ErrorBody<'a> {
    error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    issues: Option<&'a [RouteValidationIssue]>,
}
