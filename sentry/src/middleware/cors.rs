use hyper::header::{ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_REQUEST_HEADERS};
use hyper::{Body, HeaderMap, Method, Request, Response, StatusCode};

pub enum Cors {
    Preflight(Response<Body>),
    Simple(HeaderMap),
}

/// Cross-Origin Resource Sharing request handler
/// Allows all origins and methods and supports Preflighted `OPTIONS` requests.
/// On Simple CORS requests sets initial `HeaderMap` to be used in the routes handlers.
/// Otherwise returns a `OPTION` Response.
pub(crate) fn cors(req: &Request<Body>) -> Option<Cors> {
    use hyper::header::{
        HeaderValue, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN, CONTENT_LENGTH,
        ORIGIN,
    };

    // hard code the methods and origins allowed, no need for fancy iterators and etc.
    let allowed_methods = "GET,HEAD,PUT,PATCH,POST,DELETE";
    let allowed_origins = "*";

    let mut headers = req
        .headers()
        .get(ORIGIN)
        .map_or_else(Default::default, |_| {
            let mut header_map = HeaderMap::new();
            header_map.insert(
                ACCESS_CONTROL_ALLOW_ORIGIN,
                HeaderValue::from_static(allowed_origins),
            );
            header_map.insert(
                ACCESS_CONTROL_ALLOW_METHODS,
                HeaderValue::from_static(allowed_methods),
            );

            header_map
        });

    if req.method() == Method::OPTIONS {
        headers.insert(CONTENT_LENGTH, 0.into());
        // if the request has `ACCESS_CONTROL_REQUEST_HEADERS` it is required to set `ACCESS_CONTROL_ALLOW_HEADERS`
        if let Some(allow_headers) = req.headers().get(ACCESS_CONTROL_REQUEST_HEADERS) {
            headers.insert(ACCESS_CONTROL_ALLOW_HEADERS, allow_headers.clone());
        }

        let mut response = Response::builder()
            .status(StatusCode::NO_CONTENT)
            .body(Body::empty())
            .unwrap();

        // set the headers of the Request
        *response.headers_mut() = headers;

        Some(Cors::Preflight(response))
    } else if !headers.is_empty() {
        Some(Cors::Simple(headers))
    } else {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use hyper::header::{
        HeaderValue, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN,
        ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, CONTENT_LENGTH, ORIGIN,
    };
    use hyper::Request;

    #[test]
    fn check_that_simple_cors_headers_are_set_correctly() {
        let cors_req = Request::builder()
            .header(ORIGIN, "my-domain.com")
            .body(Body::empty())
            .unwrap();

        let allowed_origin_headers = match cors(&cors_req) {
            Some(Cors::Simple(headers)) => headers,
            _ => panic!("Simple CORS headers were expected"),
        };
        assert_eq!(
            "*",
            allowed_origin_headers
                .get(ACCESS_CONTROL_ALLOW_ORIGIN)
                .expect("There should be allow origin Header"),
            "Should allow all Origins"
        );
        assert_eq!(
            "GET,HEAD,PUT,PATCH,POST,DELETE",
            allowed_origin_headers
                .get(ACCESS_CONTROL_ALLOW_METHODS)
                .expect("There should be allow methods Header"),
            "Should allow all Methods"
        );
    }

    #[test]
    fn check_that_preflight_cors_request_returns_response() {
        let cors_req = Request::builder()
            // these headers should be set for a OPTION
            .header(ORIGIN, "my-domain.com")
            .header(ACCESS_CONTROL_REQUEST_METHOD, "POST")
            // if this is set in the Request, it should also be included in the Response
            // as `ACCESS_CONTROL_ALLOW_HEADERS`
            .header(ACCESS_CONTROL_REQUEST_HEADERS, "Content-Type")
            .method(Method::OPTIONS)
            .body(Body::empty())
            .unwrap();

        let response = match cors(&cors_req) {
            Some(Cors::Preflight(headers)) => headers,
            _ => panic!("Preflight CORS response was expected"),
        };

        assert_eq!(
            "Content-Type",
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_HEADERS)
                .expect("There should be allow origin Header"),
            "This header is required since we are sending a ACCESS_CONTROL_ALLOW_HEADERS"
        );
        assert_eq!(
            "GET,HEAD,PUT,PATCH,POST,DELETE",
            response
                .headers()
                .get(ACCESS_CONTROL_ALLOW_METHODS)
                .expect("There should be allow methods Header"),
            "All methods should be listed"
        );

        assert_eq!(
            HeaderValue::from(0),
            response
                .headers()
                .get(CONTENT_LENGTH)
                .expect("There should be a Content-Length set"),
            "The Content-Length should be 0 corresponding to the Status code"
        );
        assert_eq!(
            StatusCode::NO_CONTENT,
            response.status(),
            "The StatusCode should be 204 (No content)"
        );
    }

    #[test]
    fn check_that_non_cors_request_returns_none() {
        // build an empty `Request` without `ORIGIN` header nor `OPTION` method
        assert!(cors(&Default::default()).is_none());
    }
}
