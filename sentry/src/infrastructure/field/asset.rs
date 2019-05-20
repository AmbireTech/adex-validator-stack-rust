use std::error::Error;

use tokio_postgres::types::{FromSql, Type, ToSql, IsNull};

use crate::domain::Asset;

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
        <String as ToSql>::to_sql(&self.into(), ty, w)
    }

    fn accepts(ty: &Type) -> bool {
        <String as ToSql>::accepts(ty)
    }

    fn to_sql_checked(&self, ty: &Type, out: &mut Vec<u8>) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        <String as ToSql>::to_sql_checked(&self.into(), ty, out)
    }
}