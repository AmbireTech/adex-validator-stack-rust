use crate::db::DbPool;
use bb8::RunError;

pub async fn publisher_event_aggr(
    pool: &DbPool,
    channel: &Channel,
) -> Result<bool, RunError<bb8_postgres::tokio_postgres::Error>> {
    pool.run(move |connection| {
        async move {

        }
    })
}