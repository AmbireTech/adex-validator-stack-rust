use std::error::Error;

use bb8_postgres::tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

use bytes::BytesMut;
use primitives::channel::ChannelId;

#[derive(Debug)]
pub(crate) struct ChannelIdPg(ChannelId);

impl Into<ChannelId> for ChannelIdPg {
    fn into(self) -> ChannelId {
        self.0
    }
}

impl<'a> FromSql<'a> for ChannelIdPg {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<ChannelIdPg, Box<dyn Error + Sync + Send>> {
        let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

        // @TODO: Add this check back!

        //        if result.len() != 32 {
        //            return Err(DomainError::InvalidArgument(format!(
        //                "Invalid validator id value {}",
        //                value
        //            )));
        //        }

        let mut id: [u8; 32] = [0; 32];
        id.copy_from_slice(&hex::decode(str_slice)?);

        Ok(ChannelIdPg(id))
    }

    fn accepts(ty: &Type) -> bool {
        match *ty {
            Type::TEXT | Type::VARCHAR => true,
            _ => false,
        }
    }
}

impl ToSql for ChannelIdPg {
    fn to_sql(&self, ty: &Type, w: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        let string = hex::encode(&self.0);
        <String as ToSql>::to_sql(&string, ty, w)
    }

    fn accepts(ty: &Type) -> bool {
        match *ty {
            Type::TEXT | Type::VARCHAR => true,
            _ => false,
        }
    }

    fn to_sql_checked(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        let string = hex::encode(&self.0);
        <String as ToSql>::to_sql_checked(&string, ty, out)
    }
}
