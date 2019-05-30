use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset(pub(crate) String);

impl From<String> for Asset {
    fn from(asset: String) -> Self {
        Self(asset)
    }
}

impl From<&str> for Asset {
    fn from(asset: &str) -> Self {
        Self(asset.to_string())
    }
}

#[cfg(any(test, feature = "fixtures"))]
pub(crate) mod fixtures {
    use fake::helper::take_one;

    use super::Asset;

    const ASSETS_LIST: [&str; 8] = ["DAI", "BGN", "EUR", "USD", "ADX", "BTC", "LIT", "ETH"];

    pub fn get_asset() -> Asset {
        take_one(&ASSETS_LIST).into()
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use std::error::Error;

    use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

    use super::Asset;

    impl<'a> FromSql<'a> for Asset {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Asset, Box<dyn Error + Sync + Send>> {
            <String as FromSql>::from_sql(ty, raw).map(|string| string.into())
        }

        fn accepts(ty: &Type) -> bool {
            <String as FromSql>::accepts(ty)
        }
    }

    impl ToSql for Asset {
        fn to_sql(&self, ty: &Type, w: &mut Vec<u8>) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            <String as ToSql>::to_sql(&self.0, ty, w)
        }

        fn accepts(ty: &Type) -> bool {
            <String as ToSql>::accepts(ty)
        }

        fn to_sql_checked(&self, ty: &Type, out: &mut Vec<u8>) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            <String as ToSql>::to_sql_checked(&self.0, ty, out)
        }
    }
}