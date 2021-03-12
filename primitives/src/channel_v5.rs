use serde::{Serialize, Deserialize};

use crate::{ValidatorId as Address, BigNum, ChannelId, ValidatorId};
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
