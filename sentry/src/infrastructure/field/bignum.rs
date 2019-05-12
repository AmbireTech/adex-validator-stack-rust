use std::error::Error;
use std::str::FromStr;

use num_bigint::BigUint;
use tokio_postgres::types::{FromSql, Type};

use crate::domain::BigNum;

impl<'a> FromSql<'a> for BigNum {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<BigNum, Box<dyn Error + Sync + Send>> {
        let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

        let big_uint = BigUint::from_str(str_slice)?;
        Ok(BigNum::new(big_uint)?)
    }

    fn accepts(ty: &Type) -> bool {
        match *ty {
            Type::TEXT | Type::VARCHAR => true,
            _ => false,
        }
    }
}