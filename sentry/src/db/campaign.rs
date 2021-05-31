use crate::db::{DbPool, PoolError};
use primitives::Campaign;
use tokio_postgres::types::Json;

// TODO: Remove once we use this fn
#[allow(dead_code)]
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
// TODO: Remove once we use this fn
#[allow(dead_code)]
pub async fn fetch_campaign(pool: DbPool, campaign: &Campaign) -> Result<Campaign, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns WHERE id = $1").await?;

    let row = client.query_one(&statement, &[&campaign.id]).await?;

    Ok(Campaign::from(&row))
}

pub async fn get_campaigns_for_channel(pool: DbPool, campaign: &Campaign) -> Result<Vec<Campaign>, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns WHERE channel_id = $1").await?;

    let row = client.query(&statement, &[&campaign.channel.id()]).await?;

    let campaigns = row.into_iter().for_each(|c| Campaign::from(c)).collect();
    Ok(campaigns)
}

pub async fn campaign_exists(pool: &DbPool, campaign: &Campaign) -> Result<bool, PoolError> {
    let client = pool.get().await?;
    let statement = client
        .prepare("SELECT EXISTS(SELECT 1 FROM campaigns WHERE id = $1)")
        .await?;

    let row = client.execute(&statement, &[&campaign.id]).await?;

    let exists = row == 1;
    Ok(exists)
}

pub async fn update_campaign(pool: &DbPool, campaign: &Campaign) -> Result<bool, PoolError> {
    let client = pool.get().await?;
    let statement = client
        .prepare("UPDATE campaigns SET budget = $1, validators = $2, title = $3, pricing_bounds = $4, event_submission = $5, ad_units = $6, targeting_rules = $7 WHERE id = $8")
        .await?;

    let row = client
        .execute(&statement, &[
            &campaign.budget,
            &campaign.validators,
            &campaign.title,
            &campaign.pricing_bounds,
            &campaign.event_submission,
            &campaign.ad_units,
            &campaign.targeting_rules,
            &campaign.id,
        ])
        .await?;

    let exists = row == 1;
    Ok(exists)
}

#[cfg(test)]
mod test {
    use primitives::{campaign::Campaign, util::tests::prep_db::DUMMY_CAMPAIGN};

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

        let exists = campaign_exists(&db_pool.clone(), campaign: &campaign_for_testing)
            .await
            .expect("Should succeed");
        asser!(exists);

        let fetched_campaign: Campaign = fetch_campaign(db_pool.clone(), &campaign_for_testing)
            .await
            .expect("Should fetch successfully");

        assert_eq!(campaign_for_testing, fetched_campaign);
    }
}
