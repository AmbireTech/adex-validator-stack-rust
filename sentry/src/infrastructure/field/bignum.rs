use domain::BigNum;
use num_bigint::BigUint;
use std::error::Error;
use std::str::FromStr;
use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

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

impl ToSql for BigNum {
    fn to_sql(&self, ty: &Type, w: &mut Vec<u8>) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        <String as ToSql>::to_sql(&self.0.to_str_radix(10), ty, w)
    }

    fn accepts(ty: &Type) -> bool {
        match *ty {
            Type::TEXT | Type::VARCHAR => true,
            _ => false,
        }
    }

    fn to_sql_checked(&self, ty: &Type, out: &mut Vec<u8>) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        <String as ToSql>::to_sql_checked(&self.0.to_str_radix(10), ty, out)
    }
}