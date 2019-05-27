use std::convert::TryFrom;

use crate::domain::channel::ChannelId;

#[test]
fn coverts_str_to_channel_id() {
    let channel_id = ChannelId::try_from("12345678901234567890123456789012").expect("Should create ChannelId from &str with 32 len numeric value");

    assert_eq!("12345678901234567890123456789012".as_bytes(), &channel_id.id);

    assert!(ChannelId::try_from("1234567890123456789012345678901").is_err(), "ChannelId was created from a &str of 31 len bytes value");
    assert!(ChannelId::try_from("123456789012345678901234567890123").is_err(), "ChannelId was created from a &str of 33 len bytes value");
}

#[test]
fn compares_channel_ids() {
    let first = ChannelId::try_from("12345678901234567890123456789012").expect("Should create ChannelId from &str of 32 len bytes value");
    let second = ChannelId::try_from("12345678901234567890123456789012").unwrap();
    let first_copy = first;

    assert_eq!(first, first.clone(), "ChannelId and it's clone should be equal to each other");
    assert_eq!(first, first_copy, "ChannelId and it's copy should be equal to each other");
    assert_eq!(first, second, "ChannelId and it's clone should be equal to each other");
}