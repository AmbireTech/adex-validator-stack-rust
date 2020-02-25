use crate::{Application, ResponseError};
use futures::future::BoxFuture;
use hyper::{Body, Request};
use primitives::adapter::Adapter;

// chain middleware function calls
//
// function signature
// fn middleware(mut req: Request) -> Result<Request, ResponseError>
#[allow(clippy::type_complexity)]
pub async fn chain<'a, A: Adapter + 'static>(
    req: Request<Body>,
    app: &'a Application<A>,
    middlewares: Vec<
        Box<
            dyn FnMut(
                    Request<Body>,
                    &'a Application<A>,
                ) -> BoxFuture<'a, Result<Request<Body>, ResponseError>>
                + 'static
                + Send,
        >,
    >,
) -> Result<Request<Body>, ResponseError> {
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
