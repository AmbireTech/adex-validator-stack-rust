use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Copy, Clone)]
#[serde(transparent)]
pub struct Identifier {
    #[serde(with = "SerHex::<StrictPfx>")]
    pub value: [u8; 20]
}

// Will be needed later for the postgres `ToSql` implementation
impl Into<String> for &Identifier {
    /// returns the Hex string of the bytes Vec
    fn into(self) -> String {
        let hex = faster_hex::hex_string(&self.value).expect("Creating a hex string shouldn't fail at this point");

        // create the string while prefixing it with `0x`
        let mut prefixed_hex = "0x".to_string();
        prefixed_hex.push_str(&hex);
        prefixed_hex
    }
}

impl ToString for Identifier {
    fn to_string(&self) -> String {
        self.into()
    }
}

#[cfg(test)]
mod test {
    use super::Identifier;

    #[test]
    fn it_creates_a_hex_string() {
        let identifier = Identifier {
            value: *b"01234567890123456789"
        };

        let expected_string = "0x3031323334353637383930313233343536373839".to_string();

        assert_eq!(expected_string, identifier.to_string());
    }
}