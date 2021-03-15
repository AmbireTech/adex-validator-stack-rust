use serde::{Deserialize, Serialize};

use crate::{BigNum, ChannelId, ValidatorId, Address};
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: ChannelId,
    pub leader: ValidatorId,
    pub follower: ValidatorId,
    pub guardian: Address,
    pub token: Address,
    pub nonce: BigNum,
}

// TODO: Postgres Channel
