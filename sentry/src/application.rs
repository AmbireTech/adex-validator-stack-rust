use std::net::SocketAddr;

use hyper::{
    service::{make_service_fn, service_fn},
    Error, Server,
};
use primitives::adapter::Adapter;
use slog::{error, info, Logger};

use crate::Application;

/// Starts the `hyper` `Server`.
pub async fn run<A: Adapter + 'static>(app: Application<A>, socket_addr: SocketAddr) {
    let logger = app.logger.clone();
    info!(&logger, "Listening on socket address: {}!", socket_addr);

    let make_service = make_service_fn(|_| {
        let server = app.clone();
        async move {
            Ok::<_, Error>(service_fn(move |req| {
                let server = server.clone();
                async move { Ok::<_, Error>(server.handle_routing(req).await) }
            }))
        }
    });

    let server = Server::bind(&socket_addr).serve(make_service);

    if let Err(e) = server.await {
        error!(&logger, "server error: {}", e; "main" => "run");
    }
}

pub fn logger() -> Logger {
    use primitives::util::logging::{Async, PrefixedCompactFormat, TermDecorator};
    use slog::{o, Drain};

    let decorator = TermDecorator::new().build();
    let drain = PrefixedCompactFormat::new("sentry", decorator).fuse();
    let drain = Async::new(drain).build().fuse();

    Logger::root(drain, o!())
}
