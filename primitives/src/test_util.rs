use std::collections::HashMap;

use once_cell::sync::Lazy;

use crate::Address;

/// re-export all other statics before moving all of them to this module
pub use crate::util::tests::prep_db::*;

pub static LEADER: Lazy<Address> = Lazy::new(|| *ADDRESS_0);
pub static FOLLOWER: Lazy<Address> = Lazy::new(|| *ADDRESS_1);
pub static GUARDIAN: Lazy<Address> = Lazy::new(|| *ADDRESS_2);
pub static CREATOR: Lazy<Address> = Lazy::new(|| *ADDRESS_3);
pub static ADVERTISER: Lazy<Address> = Lazy::new(|| *ADDRESS_4);
pub static PUBLISHER: Lazy<Address> = Lazy::new(|| *ADDRESS_5);
pub static GUARDIAN_2: Lazy<Address> = Lazy::new(|| *ADDRESS_6);

/// passphrase: ganache0
pub static ADDRESS_0: Lazy<Address> = Lazy::new(|| {
    b"0x80690751969B234697e9059e04ed72195c3507fa"
        .try_into()
        .unwrap()
});

/// passphrase: ganache1
pub static ADDRESS_1: Lazy<Address> = Lazy::new(|| {
    b"0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7"
        .try_into()
        .unwrap()
});

/// passphrase: ganache2
pub static ADDRESS_2: Lazy<Address> = Lazy::new(|| {
    b"0xe061E1EB461EaBE512759aa18A201B20Fe90631D"
        .try_into()
        .unwrap()
});

/// passphrase: ganache3
pub static ADDRESS_3: Lazy<Address> = Lazy::new(|| {
    b"0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F"
        .try_into()
        .unwrap()
});

/// passphrase: ganache4
pub static ADDRESS_4: Lazy<Address> = Lazy::new(|| {
    b"0xDd589B43793934EF6Ad266067A0d1D4896b0dff0"
        .try_into()
        .unwrap()
});

/// passphrase: ganache5
pub static ADDRESS_5: Lazy<Address> = Lazy::new(|| {
    b"0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9"
        .try_into()
        .unwrap()
});

/// passphrase: ganache6
pub static ADDRESS_6: Lazy<Address> = Lazy::new(|| {
    b"0x79D358a3194d737880B3eFD94ADccD246af9F535"
        .try_into()
        .unwrap()
});

/// passphrase: ganache7
pub static ADDRESS_7: Lazy<Address> = Lazy::new(|| {
    b"0x0e880972A4b216906F05D67EeaaF55d16B5EE4F1"
        .try_into()
        .unwrap()
});

/// passphrase: ganache8
pub static ADDRESS_8: Lazy<Address> = Lazy::new(|| {
    b"0x541b401362Ea1D489D322579552B099e801F3632"
        .try_into()
        .unwrap()
});

/// passphrase: ganache9
pub static ADDRESS_9: Lazy<Address> = Lazy::new(|| {
    b"0x6B83e7D6B72c098d48968441e0d05658dc17Adb9"
        .try_into()
        .unwrap()
});

// Dummy adapter auth tokens
// authorization tokens
pub static DUMMY_AUTH: Lazy<HashMap<Address, String>> = Lazy::new(|| {
    let mut auth = HashMap::new();

    auth.insert(*LEADER, "AUTH_awesomeLeader".into());
    auth.insert(*FOLLOWER, "AUTH_awesomeFollower".into());
    auth.insert(*GUARDIAN, "AUTH_awesomeGuardian".into());
    auth.insert(*CREATOR, "AUTH_awesomeCreator".into());
    auth.insert(*ADVERTISER, "AUTH_awesomeAdvertiser".into());
    auth.insert(*PUBLISHER, "AUTH_awesomePublisher".into());
    auth.insert(*GUARDIAN_2, "AUTH_awesomeGuardian2".into());

    auth
});
