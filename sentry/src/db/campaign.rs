use crate::db::{DbPool, PoolError};
use primitives::{AdUnit, Campaign};
use tokio_postgres::types::Json;

pub async fn insert_campaign(pool: &DbPool, campaign: &Campaign) -> Result<bool, PoolError> {
    let client = pool.get().await?;
    let ad_units = Json(campaign.ad_units.clone());
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
                &ad_units,
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
/// SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns
/// WHERE id = $1
/// ```
pub async fn fetch_campaign(pool: DbPool, campaign: &Campaign) -> Result<Campaign, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns WHERE id = $1").await?;

    let row = client.query_one(&statement, &[&campaign.id]).await?;

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

    use crate::db::tests_postgres::{setup_test_migrations, DATABASE_POOL};

    use super::*;

    #[tokio::test]
    async fn it_inserts_and_fetches_campaign() {
        let db_pool = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(db_pool.clone())
            .await
            .expect("Migrations should succeed");

        let campaign_for_testing = DUMMY_CAMPAIGN.clone();
        let is_inserted = insert_campaign(&db_pool.clone(), &campaign_for_testing)
            .await
            .expect("Should succeed");

        assert!(is_inserted);

        let fetched_campaign: Campaign = fetch_campaign(db_pool.clone(), &campaign_for_testing)
            .await
            .expect("Should fetch successfully");

        assert_eq!(campaign_for_testing, fetched_campaign);
    }
}
