use std::collections::HashMap;

use axum::{http::StatusCode, response::IntoResponse, Json};

use primitives::sentry::ValidationErrorResponse;

#[derive(Debug, PartialEq, Eq)]
pub enum ResponseError {
    NotFound,
    BadRequest(String),
    FailedValidation(String),
    Unauthorized,
    Forbidden(String),
    Conflict(String),
    TooManyRequests(String),
}

impl IntoResponse for ResponseError {
    fn into_response(self) -> axum::response::Response {
        match self {
            ResponseError::NotFound => {
                (StatusCode::NOT_FOUND, "Not found".to_string()).into_response()
            }
            ResponseError::BadRequest(err) => {
                let error_response = [("message", err)].into_iter().collect::<HashMap<_, _>>();

                (StatusCode::BAD_REQUEST, Json(error_response)).into_response()
            }
            ResponseError::Unauthorized => {
                (StatusCode::UNAUTHORIZED, "invalid authorization").into_response()
            }
            ResponseError::FailedValidation(validator_err) => {
                let json = ValidationErrorResponse {
                    status_code: 400,
                    message: validator_err.clone(),
                    validation: vec![validator_err],
                };

                (StatusCode::BAD_REQUEST, Json(json)).into_response()
            }
            ResponseError::Forbidden(e) => (StatusCode::FORBIDDEN, e).into_response(),
            ResponseError::Conflict(e) => (StatusCode::CONFLICT, e).into_response(),
            ResponseError::TooManyRequests(e) => (StatusCode::TOO_MANY_REQUESTS, e).into_response(),
        }
    }
}

impl<T> From<T> for ResponseError
where
    T: std::error::Error + 'static,
{
    fn from(error: T) -> Self {
        ResponseError::BadRequest(error.to_string())
    }
}
