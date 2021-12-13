use adapter::{ethereum::test_util::KEYSTORES, prelude::*, Adapter, Ethereum};
use primitives::{config::GANACHE_CONFIG, test_util::LEADER};

#[tokio::test]
async fn it_creates_a_usable_adapter() {
    let client = Ethereum::init(KEYSTORES[&LEADER].clone(), &GANACHE_CONFIG)
        .expect("Should init the Ethereum Adapter");
    let adapter = Adapter::new(client);

    adapter
        .session_from_token("wrong!")
        .await
        .expect_err("Should error!");

    // `sign()` is not even callable because the adapter is locked!
    // adapter.sign("state_root").expect_err("Not callable");

    let unlocked = adapter.unlock().expect("Should unlock Adapter");

    unlocked.sign("state_root").expect("Should sign state root");
}
