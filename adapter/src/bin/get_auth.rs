use adapter::{
    ethereum::{
        test_util::{Erc20Token, Outpace},
        ChainTransport, Options,
    },
    prelude::{Locked, Unlocked},
    Adapter, Ethereum,
};
use primitives::{
    channel::Nonce,
    config::GANACHE_CONFIG,
    test_util::{CREATOR, FOLLOWER, GUARDIAN, IDS, LEADER},
    unified_num::FromWhole,
    ChainOf, Channel, UnifiedNum,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let creator_adapter = Adapter::new(Ethereum::init(Options {
        keystore_file: "./adapter/tests/resources/0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F_keystore.json".into(),
        keystore_pwd: "ganache3".into(),
    }, &GANACHE_CONFIG).unwrap()).unlock().unwrap();

    let chain_info = GANACHE_CONFIG.chains["Ganache #1337"].clone();

    // deposit().await?;
    // println!("deposit: {:#?}", creator_adapter.get_deposit(&channel_context, *CREATOR).await?);

    // println!("Auth token (intended for LEADER): {}", creator_adapter.get_auth(chain_info.chain.chain_id, IDS[&LEADER]).unwrap());
    println!(
        "Auth token (intended for FOLLOWER): {}",
        creator_adapter
            .get_auth(chain_info.chain.chain_id, IDS[&FOLLOWER])
            .unwrap()
    );

    Ok(())
}

async fn deposit() -> Result<(), Box<dyn std::error::Error>> {
    let chain_info = GANACHE_CONFIG.chains["Ganache #1337"].clone();
    let token_info = chain_info.tokens["Mocked TOKEN 1337"].clone();

    let web3_1337 = chain_info
        .chain
        .init_web3()
        .expect("Should init web3 for Chain #1");

    let token = Erc20Token::new(&web3_1337, token_info.clone());
    let outpace = Outpace::new(&web3_1337, chain_info.chain.outpace);

    let token_amount = UnifiedNum::from_whole(100).to_precision(token_info.precision.into());

    token
        .set_balance(CREATOR.to_bytes(), CREATOR.to_bytes(), &token_amount)
        .await?;

    let channel = Channel {
        leader: IDS[&LEADER],
        follower: IDS[&FOLLOWER],
        guardian: *GUARDIAN,
        token: token.info.address,
        nonce: Nonce(0.into()),
    };

    outpace
        .deposit(&channel, CREATOR.to_bytes(), &token_amount)
        .await?;

    let channel_context =
        ChainOf::new(chain_info.chain.clone(), token.info.clone()).with_channel(channel);

    Ok(())
}
