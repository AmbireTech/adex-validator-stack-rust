use futures::future::{FutureExt, TryFutureExt};
use futures_legacy::Stream;
use postgres::Client;

use crate::domain::Channel;

pub struct PostgresChannelRepository<'a> {
    client: &'a mut Client,
}

impl<'a> PostgresChannelRepository<'a> {
    pub fn new(client: &'a mut Client) -> Self {
        Self { client }
    }

    pub async fn list(&mut self) -> Result<Vec<Channel>, postgres::Error> {
        // @TODO: Add the ChannelSpecs hydration
        let statement = self.client.prepare("SELECT channel_id, creator, deposit_asset, deposit_amount, valid_until FROM channels")?;

        let result = self.client.query(&statement, &[]).unwrap()
            .iter()
            .map(|row| {
                Channel {
                    id: row.get(0),
                    creator: row.get(1),
                    deposit_asset: row.get(2),
                    deposit_amount: row.get(3),
                    valid_until: row.get(4),
                }
            }).collect();

        Ok(result)
    }
}