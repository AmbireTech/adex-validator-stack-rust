use regex::Regex;
use hyper::{Request, Body};
use crate::channel::Channel;

pub struct Path {
    matcher: Regex,
}

impl Path {
    pub fn new(path: &str) -> Path {
        let mut regex = "^".to_string();
        regex.push_str(path);
        regex.push_str("$");
        Path {
            matcher: Regex::new(&regex).unwrap(),
        }
    }
}

pub enum SentryRequest {
    ChannelList,
    ChannelCreate(Channel),
    ChannelRequest,
}

impl SentryRequest {
    pub fn from_request(request: Request<Body>) -> SentryRequest {
        SentryRequest::ChannelList
    }
}