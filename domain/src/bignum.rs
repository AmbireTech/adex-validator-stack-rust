use std::convert::TryFrom;
use std::str::FromStr;

use num_bigint::BigUint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BigNum(
    #[serde(deserialize_with = "biguint_from_str", serialize_with = "biguint_to_str")]
    pub(crate) BigUint
);

impl BigNum {
    pub fn new(num: BigUint) -> Result<Self, super::DomainError> {
        Ok(Self(num))
    }
}

impl TryFrom<u32> for BigNum {
    type Error = super::DomainError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        BigNum::new(BigUint::from(value))
    }
}

fn biguint_from_str<'de, D>(deserializer: D) -> Result<BigUint, D::Error>
    where
        D: Deserializer<'de>,
{
    let num = String::deserialize(deserializer)?;
    Ok(BigUint::from_str(&num).map_err(serde::de::Error::custom)?)
}

fn biguint_to_str<S>(num: &BigUint, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
{
    serializer.serialize_str(&num.to_str_radix(10))
}

#[cfg(feature = "postgres")]
mod postgres {
    use std::error::Error;
    use std::str::FromStr;

    use num_bigint::BigUint;
    use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

    use super::BigNum;

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
}