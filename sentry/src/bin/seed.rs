use adapter::{client::Unlocked, dummy::Options, Adapter, Dummy, UnlockedState};
use primitives::{
    config::GANACHE_CONFIG,
    sentry::SuccessResponse,
    test_util::{ADVERTISER, ADVERTISER_2, CAMPAIGNS, DUMMY_AUTH, IDS, LEADER},
    unified_num::FromWhole,
    util::ApiUrl,
    Campaign, ChainOf, Channel, Deposit, UnifiedNum, ValidatorId,
};
use reqwest::Client;
use sentry::routes::channel::ChannelDummyDeposit;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let advertiser_adapter = Adapter::with_unlocked(Dummy::init(Options {
        dummy_identity: IDS[&ADVERTISER],
        dummy_auth_tokens: DUMMY_AUTH.clone(),
        dummy_chains: GANACHE_CONFIG.chains.values().cloned().collect(),
    }));

    let advertiser2_adapter = Adapter::with_unlocked(Dummy::init(Options {
        dummy_identity: IDS[&ADVERTISER_2],
        dummy_auth_tokens: DUMMY_AUTH.clone(),
        dummy_chains: GANACHE_CONFIG.chains.values().cloned().collect(),
    }));

    let client = reqwest::Client::new();
    // add deposit for campaigns
    let intended_for = (
        "http://127.0.0.1:8005".parse::<ApiUrl>().unwrap(),
        IDS[&LEADER],
    );

    // create campaign
    // Chain 1337
    let campaign_1 = CAMPAIGNS[0].clone();
    // Chain 1337
    let campaign_2 = CAMPAIGNS[1].clone();
    // Chain 1
    let campaign_3 = CAMPAIGNS[2].clone();
    // chain 1337
    dummy_deposit(
        &client,
        &advertiser_adapter,
        &campaign_1.of_channel(),
        &intended_for,
    )
    .await?;
    // chain 1337
    dummy_deposit(
        &client,
        &advertiser_adapter,
        &campaign_2.of_channel(),
        &intended_for,
    )
    .await?;
    // chain 1
    dummy_deposit(
        &client,
        &advertiser2_adapter,
        &campaign_3.of_channel(),
        &intended_for,
    )
    .await?;

    create_campaign(&client, &advertiser_adapter, &campaign_1, &intended_for).await?;
    create_campaign(&client, &advertiser_adapter, &campaign_2, &intended_for).await?;
    create_campaign(&client, &advertiser2_adapter, &campaign_3, &intended_for).await?;

    Ok(())
}

async fn create_campaign(
    client: &Client,
    adapter: &Adapter<Dummy, UnlockedState>,
    campaign: &ChainOf<Campaign>,
    (sentry_url, intended_for): &(ApiUrl, ValidatorId),
) -> Result<(), Box<dyn std::error::Error>> {
    let auth_token = adapter.get_auth(campaign.chain.chain_id, *intended_for)?;

    let _result = client
        .post(sentry_url.join("/v5/campaign")?)
        .bearer_auth(auth_token)
        .json(&campaign.context)
        .send()
        .await?
        .error_for_status()?
        .json::<Campaign>()
        .await?;

    Ok(())
}

async fn dummy_deposit(
    client: &Client,
    adapter: &Adapter<Dummy, UnlockedState>,
    channel: &ChainOf<Channel>,
    (sentry_url, for_validator): &(ApiUrl, ValidatorId),
) -> Result<(), Box<dyn std::error::Error>> {
    let auth_token = adapter.get_auth(channel.chain.chain_id, *for_validator)?;

    let request = ChannelDummyDeposit {
        channel: channel.context,
        deposit: Deposit {
            total: UnifiedNum::from_whole(1_000_000),
        },
    };

    let result = client
        .post(sentry_url.join("/v5/channel/dummy-deposit")?)
        .bearer_auth(auth_token)
        .json(&request)
        .send()
        .await?
        .error_for_status()?
        .json::<SuccessResponse>()
        .await?;

    assert!(result.success);

    Ok(())
}
