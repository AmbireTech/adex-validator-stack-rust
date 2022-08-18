use std::collections::HashMap;

use axum::{response::IntoResponse, Json};
use hyper::{Body, Response, StatusCode};
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
impl From<ResponseError> for Response<Body> {
    fn from(response_error: ResponseError) -> Self {
        map_response_error(response_error)
    }
}

pub fn map_response_error(error: ResponseError) -> Response<Body> {
    match error {
        ResponseError::NotFound => not_found(),
        ResponseError::BadRequest(e) => bad_response(e, StatusCode::BAD_REQUEST),
        ResponseError::Unauthorized => bad_response(
            "invalid authorization".to_string(),
            StatusCode::UNAUTHORIZED,
        ),
        ResponseError::Forbidden(e) => bad_response(e, StatusCode::FORBIDDEN),
        ResponseError::Conflict(e) => bad_response(e, StatusCode::CONFLICT),
        ResponseError::TooManyRequests(e) => bad_response(e, StatusCode::TOO_MANY_REQUESTS),
        ResponseError::FailedValidation(e) => bad_validation_response(e),
    }
}

pub fn not_found() -> Response<Body> {
    let mut response = Response::new(Body::from("Not found"));
    let status = response.status_mut();
    *status = StatusCode::NOT_FOUND;
    response
}

pub fn bad_response(response_body: String, status_code: StatusCode) -> Response<Body> {
    let mut error_response = HashMap::new();
    error_response.insert("message", response_body);

    let body = Body::from(serde_json::to_string(&error_response).expect("serialize err response"));

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert("Content-type", "application/json".parse().unwrap());

    *response.status_mut() = status_code;

    response
}

pub fn bad_validation_response(response_body: String) -> Response<Body> {
    let error_response = ValidationErrorResponse {
        status_code: 400,
        message: response_body.clone(),
        validation: vec![response_body],
    };

    let body = Body::from(serde_json::to_string(&error_response).expect("serialize err response"));

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert("Content-type", "application/json".parse().unwrap());

    *response.status_mut() = StatusCode::BAD_REQUEST;

    response
}

pub fn success_response(response_body: String) -> Response<Body> {
    let body = Body::from(response_body);

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert("Content-type", "application/json".parse().unwrap());

    let status = response.status_mut();
    *status = StatusCode::OK;

    response
}
