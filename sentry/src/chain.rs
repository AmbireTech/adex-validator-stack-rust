use crate::ResponseError;
use hyper::{Body, Request};
use std::future::Future;

// chain middleware function calls
//
// function signature
// fn middleware(mut req: Request) -> Result<Request, ResponseError>

pub async fn chain<M, MF>(
    req: Request<Body>,
    middlewares: Vec<M>,
) -> Result<Request<Body>, ResponseError>
where
    MF: Future<Output = Result<Request<Body>, ResponseError>>,
    M: FnMut(Request<Body>) -> MF,
{
    middlewares
        .into_iter()
        .try_fold(req, |req, mut mw| futures::executor::block_on(mw(req)))
}
