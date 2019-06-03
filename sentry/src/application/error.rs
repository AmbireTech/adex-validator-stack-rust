#[derive(Debug)]
pub enum ApplicationError {
    NotFound,
    NoHost,
    InvalidHostValue,
    NoRedirectFound,
    InternalError,
}

impl ApplicationError {
    // @TODO: Error handling https://github.com/AdExNetwork/adex-validator-stack-rust/issues/7
    pub fn as_response(&self) -> Result<hyper::Response<hyper::Body>, http::Error> {
        hyper::Response::builder()
            .status(match self {
                ApplicationError::NoHost | ApplicationError::InvalidHostValue => {
                    hyper::StatusCode::BAD_REQUEST
                }
                ApplicationError::NoRedirectFound | ApplicationError::NotFound => {
                    hyper::StatusCode::NOT_FOUND
                }
                ApplicationError::InternalError => hyper::StatusCode::INTERNAL_SERVER_ERROR,
            })
            .body(
                match self {
                    ApplicationError::NotFound => "Route Not Found",
                    ApplicationError::NoHost => "Missing Host header",
                    ApplicationError::InvalidHostValue => "Invalid Host header",
                    ApplicationError::NoRedirectFound => "No redirect found for that host",
                    ApplicationError::InternalError => "Internal Server Error",
                }
                .into(),
            )
    }
}
