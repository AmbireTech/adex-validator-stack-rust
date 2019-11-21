use crate::{Application, ResponseError};
use hyper::{Body, Request};
use primitives::adapter::Adapter;
use std::future::Future;

// chain middleware function calls
//
// function signature
// fn middleware(mut req: Request) -> Result<Request, ResponseError>

pub async fn chain<'a, A, M, MF>(
    req: Request<Body>,
    app: &'a Application<A>,
    middlewares: Vec<M>,
) -> Result<Request<Body>, ResponseError>
where
    A: Adapter,
    MF: Future<Output = Result<Request<Body>, ResponseError>> + Send,
    M: FnMut(Request<Body>, &'a Application<A>) -> MF,
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
