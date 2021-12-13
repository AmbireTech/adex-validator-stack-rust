use crate::{db::fetch_campaign, middleware::Middleware};
use crate::{Application, Auth, ResponseError, RouteParams};
use hyper::{Body, Request};
use primitives::{adapter::Adapter, campaign::Campaign};

use async_trait::async_trait;

#[derive(Debug)]
pub struct CampaignLoad;
#[derive(Debug)]
pub struct CalledByCreator;

#[async_trait]
impl<C: adapter::client::Locked + 'static> Middleware<C> for CampaignLoad {
    async fn call<'a>(
        &self,
        mut request: Request<Body>,
        application: &'a Application<C>,
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

#[async_trait]
impl<C: adapter::client::Locked + 'static> Middleware<C> for CalledByCreator {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        _application: &'a Application<C>,
    ) -> Result<Request<Body>, ResponseError> {
        let campaign = request
            .extensions()
            .get::<Campaign>()
            .expect("We must have a campaign in extensions")
            .to_owned();

        let auth = request
            .extensions()
            .get::<Auth>()
            .expect("request should have session")
            .to_owned();

        if auth.uid.to_address() != campaign.creator {
            return Err(ResponseError::Forbidden(
                "Request not sent by campaign creator".to_string(),
            ));
        }

        Ok(request)
    }
}

#[cfg(test)]
mod test {
    use primitives::{util::tests::prep_db::DUMMY_CAMPAIGN, Campaign};

    use crate::{
        db::{insert_campaign, insert_channel},
        test_util::setup_dummy_app,
    };

    use super::*;

    #[tokio::test]
    async fn campaign_loading() {
        let app = setup_dummy_app().await;

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
            // insert Channel
            insert_channel(&app.pool, campaign.channel)
                .await
                .expect("Should insert Channel");
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
