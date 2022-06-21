//! Testing utilities for the Sentry Application

use std::ops;

use adapter::{
    dummy::{Dummy, Options},
    Adapter,
};
use primitives::{
    config::GANACHE_CONFIG,
    test_util::{discard_logger, DUMMY_AUTH, IDS, LEADER},
};

use crate::{
    db::{
        redis_pool::TESTS_POOL,
        tests_postgres::{setup_test_migrations, DATABASE_POOL},
        CampaignRemaining,
    },
    platform::PlatformApi,
    Application,
};

/// This guard holds the Redis and Postgres pools taken from their respective Pool of pools.
///
/// This ensures that they will not be dropped which will cause tests to fail randomly.
pub struct ApplicationGuard {
    pub app: Application<Dummy>,
    #[allow(dead_code)]
    redis_pool: deadpool::managed::Object<crate::db::redis_pool::Manager>,
    #[allow(dead_code)]
    db_pool: deadpool::managed::Object<crate::db::tests_postgres::Manager>,
}

impl ops::Deref for ApplicationGuard {
    type Target = Application<Dummy>;

    fn deref(&self) -> &Self::Target {
        &self.app
    }
}
impl ops::DerefMut for ApplicationGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}

/// Uses development and therefore the local ganache addresses of the tokens
/// but still uses the `DummyAdapter`.
pub async fn setup_dummy_app() -> ApplicationGuard {
    let config = GANACHE_CONFIG.clone();

    let adapter = Adapter::new(Dummy::init(Options {
        dummy_identity: IDS[&LEADER],
        dummy_auth_tokens: DUMMY_AUTH.clone(),
    }));

    let redis = TESTS_POOL.get().await.expect("Should return Object");
    let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

    setup_test_migrations(database.pool.clone())
        .await
        .expect("Migrations should succeed");

    let logger = discard_logger();

    let campaign_remaining = CampaignRemaining::new(redis.connection.clone());

    let platform_url = "http://change-me.tm".parse().expect("Bad ApiUrl!");
    let platform_api = PlatformApi::new(platform_url, config.platform.keep_alive_interval)
        .expect("should build test PlatformApi");

    let app = Application::new(
        adapter,
        config,
        logger,
        redis.connection.clone(),
        database.pool.clone(),
        campaign_remaining,
        platform_api,
    );

    ApplicationGuard {
        app,
        redis_pool: redis,
        db_pool: database,
    }
}
