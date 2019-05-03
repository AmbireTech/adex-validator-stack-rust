use hyper::Method;
use regex::Regex;

pub struct RoutePath {
    matcher: Regex,
    pub path: String,
    pub method: Method,
}

impl RoutePath {
    pub fn new(method: Method, match_path: &str) -> Self {
        let mut regex = "^".to_string();
        regex.push_str(match_path);
        regex.push_str("$");
        let matcher = Regex::new(&regex).unwrap();

        Self {
            matcher,
            path: match_path.to_string(),
            method,
        }
    }

    pub fn is_match(&self, request_path: &RequestPath) -> bool {
        &self.method == request_path.method && self.matcher.is_match(&request_path.path)
    }
}

pub struct RequestPath {
    pub path: String,
    pub method: Method,
}

impl RequestPath {
    pub fn new(method: Method, path: &str) -> Self {
        Self {
            path: path.to_string(),
            method,
        }
    }
}