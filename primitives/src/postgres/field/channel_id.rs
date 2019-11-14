use std::error::Error;

use crate::ChannelId;
use postgres_types::{FromSql, Type};

#[derive(Debug)]
pub struct ChannelIdPg(ChannelId);

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

        Ok(ChannelIdPg(id.into()))
    }

    fn accepts(ty: &Type) -> bool {
        match *ty {
            Type::TEXT | Type::VARCHAR => true,
            _ => false,
        }
    }
}
