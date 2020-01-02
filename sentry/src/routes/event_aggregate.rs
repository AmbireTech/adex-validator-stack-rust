use crate::{Application, ResponseError, Session};
use hyper::{Body, Request, Response};
use primitives::adapter::Adapter;
use primitives::Channel;

pub async fn list_channel_event_aggregates<A: Adapter>(
    req: Request<Body>,
    _app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel");

    // TODO: Auth required middleware
    let session = req
        .extensions()
        .get::<Session>()
        .ok_or_else(|| ResponseError::Unauthorized)?;

    let _is_superuser = channel.spec.validators.find(&session.uid).is_some();

    unimplemented!("Still need to finish it")
}
