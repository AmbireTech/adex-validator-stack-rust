use crate::{db::fetch_campaign, middleware::Middleware};
use crate::{Application, ResponseError, RouteParams};
use hyper::{Body, Request};
use primitives::adapter::Adapter;

use async_trait::async_trait;

#[derive(Debug)]
pub struct CampaignLoad;

#[async_trait]
impl<A: Adapter + 'static> Middleware<A> for CampaignLoad {
    async fn call<'a>(
        &self,
        mut request: Request<Body>,
        application: &'a Application<A>,
    ) -> Result<Request<Body>, ResponseError> {
        let id = request
            .extensions()
            .get::<RouteParams>()
            .ok_or_else(|| ResponseError::BadRequest("Route params not found".to_string()))?
            .get(0)
            .ok_or_else(|| ResponseError::BadRequest("No id".to_string()))?;

        let campaign_id = id
            .parse()
            .map_err(|_| ResponseError::BadRequest("Wrong Campaign Id".to_string()))?;
        let campaign = fetch_campaign(application.pool.clone(), &campaign_id)
            .await?
            .ok_or(ResponseError::NotFound)?;

        request.extensions_mut().insert(campaign);

        Ok(request)
    }
}

#[cfg(test)]
mod test {
    use adapter::DummyAdapter;
    use primitives::{
        adapter::DummyAdapterOptions,
        config::configuration,
        util::tests::{
            discard_logger,
            prep_db::{DUMMY_CAMPAIGN, IDS},
        },
        Campaign,
    };

    use crate::db::{
        insert_campaign,
        redis_pool::TESTS_POOL,
        tests_postgres::{setup_test_migrations, DATABASE_POOL},
    };

    use super::*;

    async fn setup_app() -> Application<DummyAdapter> {
        let config = configuration("development", None).expect("Should get Config");
        let adapter = DummyAdapter::init(
            DummyAdapterOptions {
                dummy_identity: IDS["leader"],
                dummy_auth: Default::default(),
                dummy_auth_tokens: Default::default(),
            },
            &config,
        );

        let redis = TESTS_POOL.get().await.expect("Should return Object");
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let app = Application::new(
            adapter,
            config,
            discard_logger(),
            redis.connection.clone(),
            database.pool.clone(),
        );

        app
    }

    #[tokio::test]
    async fn campaign_loading() {
        let app = setup_app().await;

        let build_request = |params: RouteParams| {
            Request::builder()
                .extension(params)
                .body(Body::empty())
                .expect("Should build Request")
        };

        let campaign = DUMMY_CAMPAIGN.clone();

        let campaign_load = CampaignLoad;

        // bad CampaignId
        {
            let route_params = RouteParams(vec!["Bad campaign Id".to_string()]);

            let res = campaign_load
                .call(build_request(route_params), &app)
                .await
                .expect_err("Should return error for Bad Campaign");

            assert_eq!(
                ResponseError::BadRequest("Wrong Campaign Id".to_string()),
                res
            );
        }

        let route_params = RouteParams(vec![campaign.id.to_string()]);
        // non-existent campaign
        {
            let res = campaign_load
                .call(build_request(route_params.clone()), &app)
                .await
                .expect_err("Should return error for Not Found");

            assert!(matches!(res, ResponseError::NotFound));
        }

        // existing Campaign
        {
            // insert Campaign
            assert!(insert_campaign(&app.pool, &campaign)
                .await
                .expect("Should insert Campaign"));

            let request = campaign_load
                .call(build_request(route_params), &app)
                .await
                .expect("Should load campaign");

            assert_eq!(Some(&campaign), request.extensions().get::<Campaign>());
        }
    }
}
