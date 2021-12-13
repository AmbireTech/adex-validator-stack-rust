use parse_display::Display;
use std::{error::Error as StdError, fmt};
use thiserror::Error;

pub(crate) type BoxError = Box<dyn StdError + Send + Sync>;

#[derive(Debug, Error)]
#[error("{inner}")]
pub struct Error {
    inner: Box<Inner>,
}

impl Error {
    pub(crate) fn new<E>(kind: Kind, source: Option<E>) -> Self
    where
        E: Into<BoxError>,
    {
        Self {
            inner: Box::new(Inner {
                kind,
                source: source.map(Into::into),
            }),
        }
    }

    pub fn wallet_unlock<E>(source: E) -> Self
    where
        E: Into<BoxError>,
    {
        Self::new(Kind::WalletUnlock, Some(source))
    }

    pub fn authentication<E>(source: E) -> Self
    where
        E: Into<BoxError>,
    {
        Self::new(Kind::Authentication, Some(source))
    }

    pub fn authorization<E>(source: E) -> Self
    where
        E: Into<BoxError>,
    {
        Self::new(Kind::Authorization, Some(source))
    }

    pub fn adapter<A>(source: A) -> Self
    where
        A: Into<BoxError>,
    {
        Self::new(Kind::Adapter, Some(source))
    }

    pub fn verify<A>(source: A) -> Self
    where
        A: Into<BoxError>,
    {
        Self::new(Kind::Verify, Some(source))
    }
}
#[derive(Debug, Error)]
struct Inner {
    kind: Kind,
    source: Option<BoxError>,
}

impl fmt::Display for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.source {
            // Writes: "Kind: Error message here"
            Some(source) => write!(f, "{}: {}", self.kind, source.to_string()),
            // Writes: "Kind"
            None => write!(f, "{}", self.kind),
        }
    }
}

#[derive(Debug, Display)]
pub(crate) enum Kind {
    Adapter,
    WalletUnlock,
    Verify,
    Authentication,
    Authorization,
}
