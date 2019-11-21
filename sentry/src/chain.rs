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
    MF: Future<Output = Result<Request<Body>, ResponseError>> + Send,
    M: FnMut(Request<Body>) -> MF,
{
    let mut req = Ok(req);

    for mut mw in middlewares.into_iter() {
        match mw(req.unwrap()).await {
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
