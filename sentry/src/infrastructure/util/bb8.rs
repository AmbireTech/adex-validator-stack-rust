use futures_legacy::future::IntoFuture;
use futures_legacy::stream::Stream;
use futures_legacy::Future;
use tokio_postgres::impls::Query;
use tokio_postgres::{Client, Row};
use try_future::TryFuture;

pub(crate) fn query_result(
    query: Query,
    client: Client,
) -> TryFuture<impl Future<Item = (Vec<Row>, Client), Error = (tokio_postgres::Error, Client)>> {
    query
        .collect()
        .into_future()
        .then(|res| match res {
            Ok(rows) => Ok((rows, client)),
            Err(err) => Err((err, client)),
        })
        .into_future()
        .into()
}
