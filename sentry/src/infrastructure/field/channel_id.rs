use std::error::Error;

use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

use domain::ChannelId;

#[derive(Debug)]
pub(crate) struct ChannelIdPg(ChannelId);

impl Into<ChannelId> for ChannelIdPg {
    fn into(self) -> ChannelId {
        self.0
    }
}

impl ToString for &ChannelIdPg {
    fn to_string(&self) -> String {
        self.0.to_string()
    }
}

impl<'a> FromSql<'a> for ChannelIdPg {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<ChannelIdPg, Box<dyn Error + Sync + Send>> {
        <String as FromSql>::from_sql(ty, raw)
            .map(|string| {
                let channel_id = ChannelId::try_from_hex(&string)?;
                Ok(ChannelIdPg(channel_id))
            })?
    }

    fn accepts(ty: &Type) -> bool {
        <String as FromSql>::accepts(ty)
    }
}

impl ToSql for ChannelIdPg {
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
