use std::error::Error;

use crate::ChannelId;
use hex::FromHex;
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

        // FromHex::from_hex for fixed-sized arrays will guard against the length of the string!
        let id: [u8; 32] = <[u8; 32] as FromHex>::from_hex(str_slice)?;

        Ok(ChannelIdPg(id.into()))
    }

    fn accepts(ty: &Type) -> bool {
        match *ty {
            Type::TEXT | Type::VARCHAR => true,
            _ => false,
        }
    }
}
