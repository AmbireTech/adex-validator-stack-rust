use tiny_keccak::{Hasher, Keccak};

pub fn checksum(address: &str) -> String {
    let address = address.trim_start_matches("0x").to_lowercase();

    let address_hash = {
        let mut hasher = Keccak::v256();
        let mut result: [u8; 32] = [0; 32];

        hasher.update(address.as_bytes());
        hasher.finalize(&mut result);

        hex::encode(result)
    };

    address
        .char_indices()
        .fold(String::from("0x"), |mut acc, (index, address_char)| {
            // this cannot fail since it's Keccak256 hashed
            let n = u16::from_str_radix(&address_hash[index..index + 1], 16).unwrap();

            if n > 7 {
                // make char uppercase if ith character is 9..f
                acc.push_str(&address_char.to_uppercase().to_string())
            } else {
                // already lowercased
                acc.push(address_char)
            }

            acc
        })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_checksums() {
        let expected_checksum = "0xce07CbB7e054514D590a0262C93070D838bFBA2e";

        let non_checksummed = expected_checksum.to_lowercase();

        assert_eq!(expected_checksum, checksum(&non_checksummed));

        let non_prefixed = non_checksummed
            .strip_prefix("0x")
            .expect("should have prefix");

        assert_eq!(expected_checksum, checksum(&non_prefixed))
    }
}
