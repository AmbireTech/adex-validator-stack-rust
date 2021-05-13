use crate::db::{DbPool, PoolError};
use primitives::{channel::ChannelId, Campaign};
use std::convert::TryFrom;

pub async fn insert_campaign(pool: &DbPool, campaign: &Campaign) -> Result<bool, PoolError> {
    let client = pool.get().await?;

    let stmt = client.prepare("INSERT INTO campaigns (id, channel_id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to) values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)").await?;

    let row = client
        .execute(
            &stmt,
            &[
                &campaign.id,
                &campaign.channel.id(),
                &campaign.channel,
                &campaign.creator,
                &campaign.budget,
                &campaign.validators,
                &campaign.title,
                &campaign.pricing_bounds,
                &campaign.event_submission,
                &campaign.ad_units,
                &campaign.targeting_rules,
                &campaign.created,
                &campaign.active.from,
                &campaign.active.to,
            ],
        )
        .await?;

    let inserted = row == 1;
    Ok(inserted)
}

/// ```text
/// SELECT id, channel_id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns
/// WHERE id = $1 AND channel_id = $2
/// ```
pub async fn fetch_campaign(pool: DbPool, campaign: &Campaign) -> Result<Campaign, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT id, channel_id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns WHERE id = $1 AND channel_id = $2").await?;

    let row = client
        .query_one(
            &statement,
            &[&campaign.id, &ChannelId::from(campaign.channel.id())],
        )
        .await?;

    Ok(Campaign::from(&row))
}

#[cfg(test)]
mod test {
    use primitives::{
        campaign::{Campaign, CampaignId},
        channel_v5::Channel,
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        ChannelId, UnifiedNum,
    };

    use crate::db::{
        tests_postgres::{setup_test_migrations, test_postgres_connection},
        POSTGRES_CONFIG,
    };

    use super::*;

    #[tokio::test]
    async fn it_inserts_and_fetches_campaign() {
        let test_pool = test_postgres_connection(POSTGRES_CONFIG.clone())
            .get()
            .await
            .unwrap();

        setup_test_migrations(test_pool.clone())
            .await
            .expect("Migrations should succeed");

        let campaign_for_testing = DUMMY_CAMPAIGN.clone();
        let is_inserted = insert_campaign(&test_pool.clone(), &campaign_for_testing)
            .await
            .expect("Should succeed");

        assert!(is_inserted);

        let fetched_campaign: Campaign = fetch_campaign(test_pool.clone(), &campaign_for_testing)
            .await
            .expect("Should fetch successfully");

        assert_eq!(campaign_for_testing, fetched_campaign);
    }
}
