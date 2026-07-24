//! Map [`XJError`] to gRPC [`tonic::Status`] (paridad Node `mapGrpcError`).

use tonic::Code;
use tonic::Status;

use crate::error::XJError;

pub fn map_grpc_error(error: &XJError) -> Status {
    let code = match error {
        XJError::BadRequest { .. } | XJError::ValidationBadRequest { .. } => Code::InvalidArgument,
        XJError::Forbidden { .. } => Code::PermissionDenied,
        XJError::NotFound { .. } => Code::NotFound,
        XJError::TooManyRequests => Code::ResourceExhausted,
        XJError::InternalServerError { .. } => Code::Internal,
    };

    let details = error.to_json_body().to_string();
    Status::with_details(code, details.clone(), details.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::RouteValidationIssue;

    #[test]
    fn maps_validation_to_invalid_argument() {
        let err = XJError::validation_bad_request(
            "Validation failed",
            vec![RouteValidationIssue {
                path: "/user".into(),
                message: "required".into(),
                code: None,
            }],
        );
        let status = map_grpc_error(&err);
        assert_eq!(status.code(), Code::InvalidArgument);
        assert!(status.message().contains("issues"));
    }

    #[test]
    fn maps_forbidden() {
        let status = map_grpc_error(&XJError::forbidden("nope"));
        assert_eq!(status.code(), Code::PermissionDenied);
    }

    #[test]
    fn maps_rate_limit() {
        let status = map_grpc_error(&XJError::TooManyRequests);
        assert_eq!(status.code(), Code::ResourceExhausted);
    }
}
