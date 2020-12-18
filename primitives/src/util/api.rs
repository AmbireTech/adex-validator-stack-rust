use std::{convert::TryFrom, fmt, str::FromStr};

use parse_display::Display;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

// `url::Url::scheme()` returns lower-cased ASCII string without `:`
const SCHEMES: [&str; 2] = ["http", "https"];

#[derive(Debug, Error, PartialEq)]
pub enum Error {
    #[error("Invalid scheme '{0}', only 'http' & 'https' are allowed")]
    InvalidScheme(String),
    #[error("The Url has to be a base, i.e. `data:`, `mailto:` etc. are not allowed")]
    ShouldBeABase,
    #[error("Having a fragment (i.e. `#fragment`) is not allowed")]
    HasFragment,
    #[error("Having a query parameters (i.e. `?query_param=value`) is not allowed")]
    HasQuery,
    #[error("Parsing the url: {0}")]
    Parsing(#[from] url::ParseError),
}

/// A safe Url to use in REST API calls.
///
/// It makes sure to always end the Url with `/`,
/// however it doesn't check for the existence of a file, e.g. `/path/a-file.html`
///
/// Underneath it uses [`url::Url`], so all the validation from there is enforced,
/// with additional validation which doesn't allow having:
/// - `Scheme` different that `http` & `https`
/// - Non-base `url`s like `data:` & `mailto:`
/// - `Fragment`, e.g. `#fragment`
/// - `Query`, e.g. `?query_param=value`, `?query_param`, `?query=value&....`, etc.
///
/// [`url::Url`]: url::Url
#[derive(Clone, Hash, Display, Ord, PartialOrd, Eq, PartialEq, Deserialize, Serialize)]
#[serde(try_from = "Url", into = "Url")]
pub struct ApiUrl(Url);

impl ApiUrl {
    pub fn parse(input: &str) -> Result<Self, Error> {
        Self::from_str(input)
    }

    /// The Endpoint of which we want to get an url to (strips prefixed `/` from the endpoint),
    /// which can can include:
    /// - path
    /// - query
    /// - fragments - usually should not be used for requesting API resources from server
    /// This method does **not** check if a file is present
    /// This method strips the starting `/` of the endpoint, if there is one
    pub fn join(&self, endpoint: &str) -> Result<Url, url::ParseError> {
        let stripped = endpoint.strip_prefix('/').unwrap_or(endpoint);
        // this join is safe, since we always prefix the Url with `/`
        self.0.join(stripped)
    }

    pub fn to_url(&self) -> Url {
        self.0.clone()
    }
}

impl fmt::Debug for ApiUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Url({})", self)
    }
}

impl TryFrom<Url> for ApiUrl {
    type Error = Error;

    fn try_from(mut url: Url) -> Result<Self, Self::Error> {
        if url.cannot_be_a_base() {
            return Err(Error::ShouldBeABase);
        }

        if url.fragment().is_some() {
            return Err(Error::HasFragment);
        }

        if !SCHEMES.contains(&url.scheme()) {
            return Err(Error::InvalidScheme(url.scheme().to_string()));
        }

        if url.query().is_some() {
            return Err(Error::HasQuery);
        }

        let url_path = url.path();

        let mut stripped_path = url_path.strip_suffix('/').unwrap_or(url_path).to_string();
        // Make sure to always end the path with `/`!
        stripped_path.push('/');

        url.set_path(&stripped_path);

        Ok(Self(url))
    }
}

impl Into<Url> for ApiUrl {
    fn into(self) -> Url {
        self.0
    }
}

impl FromStr for ApiUrl {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s.parse::<Url>()?)
    }
}

#[cfg(test)]
mod test {
    use url::ParseError;

    use super::*;

    #[test]
    fn api_url() {
        let allowed = vec![
            // Http
            (
                "http://127.0.0.1/",
                ApiUrl::from_str("http://127.0.0.1/").unwrap(),
            ),
            (
                "http://127.0.0.1",
                ApiUrl::from_str("http://127.0.0.1/").unwrap(),
            ),
            // Https
            (
                "https://127.0.0.1/",
                ApiUrl::from_str("https://127.0.0.1/").unwrap(),
            ),
            (
                "https://127.0.0.1",
                ApiUrl::from_str("https://127.0.0.1/").unwrap(),
            ),
            // Domain `/` suffixed
            (
                "https://jerry.adex.network/",
                ApiUrl::from_str("https://jerry.adex.network/").unwrap(),
            ),
            (
                "https://tom.adex.network",
                ApiUrl::from_str("https://tom.adex.network/").unwrap(),
            ),
            // With Port
            (
                "https://localhost:3335",
                ApiUrl::from_str("https://localhost:3335/").unwrap(),
            ),
            // With Path non `/` suffixed
            (
                "https://localhost/leader",
                ApiUrl::from_str("https://localhost/leader/").unwrap(),
            ),
            // With Path `/` suffixed
            (
                "https://localhost/leader/",
                ApiUrl::from_str("https://localhost/leader/").unwrap(),
            ),
            // with authority
            (
                "https://username:password@localhost",
                ApiUrl::from_str("https://username:password@localhost/").unwrap(),
            ),
            // HTTPS, authority, domain, port and path
            (
                "https://username:password@jerry.adex.network:3335/leader",
                ApiUrl::from_str("https://username:password@jerry.adex.network:3335/leader")
                    .unwrap(),
            ),
        ];

        let failing = vec![
            // Unix socket
            (
                "unix:/run/foo.socket",
                Error::InvalidScheme("unix".to_string()),
            ),
            // A file URL
            (
                "file://127.0.0.1/",
                Error::InvalidScheme("file".to_string()),
            ),
            // relative path
            (
                "/relative/path",
                Error::Parsing(ParseError::RelativeUrlWithoutBase),
            ),
            (
                "/relative/path/",
                Error::Parsing(ParseError::RelativeUrlWithoutBase),
            ),
            // blob
            ("data:text/plain,Stuff", Error::ShouldBeABase),
        ];

        for (case, expected) in allowed {
            let url = case.parse::<ApiUrl>();
            assert_eq!(url, Ok(expected))
        }

        for (case, error) in failing {
            assert_eq!(case.parse::<ApiUrl>(), Err(error))
        }
    }

    #[test]
    fn api_endpoint() {
        let api_url = ApiUrl::parse("http://127.0.0.1/leader").expect("It is a valid API URL");

        let expected = url::Url::parse("http://127.0.0.1/leader/endpoint?query=query value")
            .expect("it is a valid Url");
        let expected_url_encoded = "http://127.0.0.1/leader/endpoint?query=query%20value";

        let actual = api_url
            .join("endpoint?query=query value")
            .expect("Should join endpoint");
        let actual_should_strip_suffix = api_url
            .join("/endpoint?query=query value")
            .expect("Should join endpoint and strip `/` suffix and preserve the original ApiUrl");
        assert_eq!(&expected, &actual);
        assert_eq!(&expected, &actual_should_strip_suffix);

        assert_eq!(expected_url_encoded, &actual.to_string());
        assert_eq!(
            expected_url_encoded,
            &actual_should_strip_suffix.to_string()
        );
    }
}
