use serde::{Deserialize, Serialize};

use std::{convert::TryFrom, fmt, str::FromStr};
use thiserror::Error;

const URL_PREFIX: &str = "ipfs://";

pub use cid::{Cid, Error};

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct IPFS(pub cid::Cid);

impl fmt::Debug for IPFS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IPFS({})", self.0.to_string())
    }
}

impl fmt::Display for IPFS {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl FromStr for IPFS {
    type Err = cid::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl Into<String> for IPFS {
    fn into(self) -> String {
        self.0.into()
    }
}

impl TryFrom<String> for IPFS {
    type Error = cid::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl TryFrom<&String> for IPFS {
    type Error = cid::Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl<'a> TryFrom<&'a str> for IPFS {
    type Error = cid::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        cid::Cid::try_from(value).map(Self)
    }
}

#[derive(Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(try_from = "String", into = "String")]
pub struct Url(IPFS);

impl Url {
    pub fn to_url(&self) -> url::Url {
        self.into()
    }

    pub fn into_ipfs(self) -> IPFS {
        self.0
    }

    pub fn as_ipfs(&self) -> &IPFS {
        &self.0
    }
}

#[derive(Debug, Clone, Error, Eq, PartialEq)]
pub enum UrlError {
    #[error("Parsing the IPFS Cid failed")]
    IPFS(#[from] cid::Error),
    #[error("Url should start with {} prefix", URL_PREFIX)]
    NoPrefix,
}

impl Into<url::Url> for Url {
    fn into(self) -> url::Url {
        (&self).into()
    }
}

impl Into<url::Url> for &Url {
    fn into(self) -> url::Url {
        let url_string = self.to_string();

        url::Url::parse(&url_string).expect("This should never fail")
    }
}

impl Into<String> for Url {
    fn into(self) -> String {
        self.to_string()
    }
}

impl Into<String> for &Url {
    fn into(self) -> String {
        self.to_string()
    }
}

impl From<IPFS> for Url {
    fn from(ipfs: IPFS) -> Self {
        Self(ipfs)
    }
}

impl FromStr for Url {
    type Err = UrlError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::try_from(s)
    }
}

impl TryFrom<String> for Url {
    type Error = UrlError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl TryFrom<&str> for Url {
    type Error = UrlError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.strip_prefix("ipfs://") {
            // There is a prefix, everything is OK
            Some(stripped) => Ok(Self(IPFS::try_from(stripped)?)),
            None => Err(UrlError::NoPrefix),
        }
    }
}

impl TryFrom<url::Url> for Url {
    type Error = UrlError;

    fn try_from(url: url::Url) -> Result<Self, Self::Error> {
        Self::try_from(url.to_string())
    }
}

impl fmt::Display for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let url = URL_PREFIX.to_string() + self.0.to_string().as_str();

        write!(f, "{}", url)
    }
}

impl fmt::Debug for Url {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Url({})", self.to_string())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    // CID V0
    static TESTS_IPFS_V0: [&str; 4] = [
        "QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR",
        "Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f",
        "QmQnu8zrHsuVvnTJsEgDHYA8c1MmRL7YLiMD8uzDUJKcNq",
        "QmYYBULc9QDEaDr8HAXvVWHDmFfL2GvyumYRr1g4ERBC96",
    ];

    // CID V1
    static TESTS_IPFS_V1: [&str; 1] = [
        // V1 of the V0 ipfs: `QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR`
        "bafybeif2h3mynaf3ylgdbs6arf6mczqycargt5cqm3rmel3wpjarlswway",
    ];

    #[test]
    fn ipfs_from_string_and_serialize_deserialize() {
        let check = |ipfs_str: &str, version: cid::Version| {
            let ipfs = IPFS::try_from(ipfs_str).expect("should be ok");
            assert_eq!(ipfs.0.version(), version);
            assert_eq!(ipfs.0.to_string(), ipfs_str);

            let expected_json = format!("\"{}\"", ipfs);
            let actual_json = serde_json::to_string(&ipfs).expect("Should serialize");

            assert_eq!(expected_json, actual_json);
            assert_eq!(
                ipfs,
                serde_json::from_str(&actual_json).expect("Should Deserialize")
            )
        };

        for &ipfs_str in TESTS_IPFS_V0.iter() {
            check(ipfs_str, cid::Version::V0)
        }

        for &ipfs_str in TESTS_IPFS_V1.iter() {
            check(ipfs_str, cid::Version::V1)
        }

        // v0 != v1
        assert_ne!(
            IPFS::try_from(TESTS_IPFS_V0[0]),
            IPFS::try_from(TESTS_IPFS_V1[0])
        )
    }

    #[test]
    fn url_from_string_serialize_deserialize_and_into_and_from_url() {
        // Valid cases
        for &ipfs_str in TESTS_IPFS_V1.iter() {
            let url_string = format!("ipfs://{}", ipfs_str);

            let url = Url::try_from(url_string.as_str())
                .expect("Should create from valid ipfs:// prefixed URL");

            assert_eq!(&url_string, &url.to_string());
            assert_eq!(
                url::Url::from_str(&url_string).expect("Valid url::Url provided"),
                url.to_url()
            );

            assert_eq!(&url, &url_string.parse::<Url>().expect("Should parse"));

            let expected_json = format!("\"{}\"", url);
            let actual_json = serde_json::to_string(&url).expect("Should serialize");

            assert_eq!(expected_json, actual_json);
            assert_eq!(
                url,
                serde_json::from_str(&actual_json).expect("Should Deserialize")
            )
        }

        // Invalid cases
        // CID V0 - Invalid scheme - valid IPFS
        assert_eq!(
            Err(UrlError::NoPrefix),
            "https://QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR".parse::<Url>()
        );
        // CID V0 - Invalid scheme - valid IPFS
        assert_eq!(
            Err(UrlError::IPFS(cid::Error::ParsingError)),
            "ipfs://NotValid".parse::<Url>()
        );
        // CID V1 - Invalid scheme - valid IPFS
        assert_eq!(
            Err(UrlError::NoPrefix),
            "https://bafybeif2h3mynaf3ylgdbs6arf6mczqycargt5cqm3rmel3wpjarlswway".parse::<Url>()
        );
    }
}
