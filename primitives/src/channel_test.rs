//use std::convert::TryFrom;
//
//use crate::channel::ChannelId;
//
//#[test]
//fn coverts_str_to_channel_id() {
//    let channel_id = ChannelId::try_from("12345678901234567890123456789012")
//        .expect("Should create ChannelId from &str with 32 len numeric value");
//
//    assert_eq!(b"12345678901234567890123456789012", &channel_id.bytes);
//
//    assert!(
//        ChannelId::try_from("1234567890123456789012345678901").is_err(),
//        "ChannelId was created from a &str of 31 len bytes value"
//    );
//    assert!(
//        ChannelId::try_from("123456789012345678901234567890123").is_err(),
//        "ChannelId was created from a &str of 33 len bytes value"
//    );
//}
//
//#[test]
//fn compares_channel_ids() {
//    let first = ChannelId::try_from("12345678901234567890123456789012")
//        .expect("Should create ChannelId from &str of 32 len bytes value");
//    let second = ChannelId::try_from("12345678901234567890123456789012").unwrap();
//    let first_copy = first;
//
//    assert_eq!(
//        first,
//        first.clone(),
//        "ChannelId and it's clone should be equal to each other"
//    );
//    assert_eq!(
//        first, first_copy,
//        "ChannelId and it's copy should be equal to each other"
//    );
//    assert_eq!(
//        first, second,
//        "ChannelId and it's clone should be equal to each other"
//    );
//}
//
//#[test]
//fn serialize_and_deserialize_channel_id() {
//    let channel_id_str = "01234567890123456789012345678901";
//    let channel_id = ChannelId::try_from(channel_id_str).unwrap();
//    let serialized = serde_json::to_string(&channel_id).unwrap();
//
//    // with Hex value of `01234567890123456789012345678901`
//    let expected_json = r#""0x3031323334353637383930313233343536373839303132333435363738393031""#;
//
//    assert_eq!(expected_json, serialized);
//
//    let from_hex: ChannelId = serde_json::from_str(expected_json).unwrap();
//    assert_eq!(from_hex, channel_id);
//}
