use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Copy, Clone)]
pub struct Identifier(
    #[serde(with = "SerHex::<StrictPfx>")] [u8; 20]
);

// Will be needed later for the postgres `ToSql` implementation
impl Into<String> for &Identifier {
    /// returns the Hex string of the bytes Vec
    fn into(self) -> String {
        let hex = faster_hex::hex_string(&self.0).expect("Creating a hex string shouldn't fail at this point");

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
pub(crate) mod fixtures {
    use super::Identifier;

    /// Creates an identifier where the passed id is prefixed with enough b'0' to make it to 20 bytes
    pub fn get_identifier(identifier: &str) -> Identifier {
        let identifier_bytes = identifier.as_bytes();
        if identifier_bytes.len() > 20 {
            panic!("The passed &str should be <= 20 bytes");
        }

        let mut id: [u8; 20] = [b'0'; 20];
        for (index, byte) in id[20 - identifier.len()..].iter_mut().enumerate() {
            *byte = identifier_bytes[index];
        }

        Identifier(id)
    }
}

#[cfg(test)]
mod test {
    use super::Identifier;

    #[test]
    fn it_creates_a_hex_string() {
        let identifier = Identifier(*b"01234567890123456789");

        let expected_string = "0x3031323334353637383930313233343536373839".to_string();

        assert_eq!(expected_string, identifier.to_string());
    }

    #[test]
    fn it_serializes_an_deserializes() {
        let identifier = Identifier(*b"01234567890123456789");

        let serialized = serde_json::to_string(&identifier).unwrap();

        let expected_json = r#""0x3031323334353637383930313233343536373839""#;

        assert_eq!(expected_json, serialized);

        let from_hex: Identifier = serde_json::from_str(expected_json).unwrap();
        assert_eq!(from_hex, identifier);
    }
}