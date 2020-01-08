use crate::{Application, ResponseError};
use futures::future::BoxFuture;
use hyper::{Body, Request};
use primitives::adapter::Adapter;

// chain middleware function calls
//
// function signature
// fn middleware(mut req: Request) -> Result<Request, ResponseError>

pub async fn chain<'a, A: Adapter + 'static, M>(
    req: Request<Body>,
    app: &'a Application<A>,
    middlewares: Vec<M>,
) -> Result<Request<Body>, ResponseError>
where
    M: FnMut(
            Request<Body>,
            &'a Application<A>,
        ) -> BoxFuture<'a, Result<Request<Body>, ResponseError>>
        + 'static,
{
    let mut req = Ok(req);

    for mut mw in middlewares.into_iter() {
        match mw(req.unwrap(), app).await {
            Ok(r) => {
                req = Ok(r);
            }
            Err(e) => {
                req = Err(e);
                break;
            }
        }
    }

    req
}
