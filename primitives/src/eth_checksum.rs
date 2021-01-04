use crypto::{digest::Digest, sha3::Sha3};

pub fn checksum(address: &str) -> String {
    let address = address.trim_start_matches("0x").to_lowercase();

    let address_hash = {
        let mut hasher = Sha3::keccak256();
        hasher.input(address.as_bytes());
        hasher.result_str()
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
