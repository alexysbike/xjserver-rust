use axum::Json;
use axum::response::{IntoResponse, Response};
use http::StatusCode;

use crate::error::XJError;

impl IntoResponse for XJError {
    fn into_response(self) -> Response {
        let status =
            StatusCode::from_u16(self.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = Json(self.to_json_body());
        (status, body).into_response()
    }
}
