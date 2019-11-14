use hyper::{Body, Request, Response};
use std::future::Future;
use crate::ResponseError;

// middleware function signature
// fn middleware(mut req: Request) -> Result<Request, ResponseError>
//
// handler function signature
// fn middleware(mut req: Request) -> Result<Response<Body>, ResponseError>
pub async fn chain<M, H, MF, HF>(req: Request<Body>, middlewares: Option<Vec<M>>, handler: H) -> Result<Response<Body>, ResponseError> 
where
    HF: Future<Output=Result<Response<Body>, ResponseError>>,
    MF: Future<Output=Result<Request<Body>, ResponseError>>,
    H: Fn(Request<Body>) -> HF,
    M: FnMut(Request<Body>) -> MF,
{
    if let Some(mws) = middlewares {
        let request = mws.into_iter().try_fold(req, |req, mut mw| futures::executor::block_on(mw(req)));
        if let Err(e) = request {
            return Err(e);
        }
        handler(request.unwrap()).await
    } else {
        handler(req).await
    }
}
