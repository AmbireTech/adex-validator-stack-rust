use std::error::Error;

use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

use domain::Asset;

#[derive(Debug)]
pub(crate) struct AssetPg(Asset);

impl Into<Asset> for AssetPg {
    fn into(self) -> Asset {
        self.0
    }
}

impl ToString for &AssetPg {
    fn to_string(&self) -> String {
        self.0.to_owned().into()
    }
}

impl<'a> FromSql<'a> for AssetPg {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<AssetPg, Box<dyn Error + Sync + Send>> {
        <String as FromSql>::from_sql(ty, raw).map(|string| AssetPg(string.into()))
    }

    fn accepts(ty: &Type) -> bool {
        <String as FromSql>::accepts(ty)
    }
}

impl ToSql for AssetPg {
    fn to_sql(&self, ty: &Type, w: &mut Vec<u8>) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        let string = self.to_string();
        <String as ToSql>::to_sql(&string, ty, w)
    }

    fn accepts(ty: &Type) -> bool {
        <String as ToSql>::accepts(ty)
    }

    fn to_sql_checked(&self, ty: &Type, out: &mut Vec<u8>) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        let string = self.to_string();

        <String as ToSql>::to_sql_checked(&string, ty, out)
    }
}
