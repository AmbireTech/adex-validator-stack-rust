use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{config::TokenInfo, util::ApiUrl, Address, Campaign, Channel};

/// The Id of the chain
///
/// # Ethereum Virtual Machine
///
/// For all the EVM-compatible Chain IDs visit <https://chainid.network>
#[derive(Serialize, Deserialize, Hash, Clone, Copy, Eq, PartialEq)]
#[serde(transparent)]
pub struct ChainId(u32);

impl ChainId {
    /// # Panics:
    ///
    /// If `id` is `0`.
    pub fn new(id: u32) -> Self {
        assert!(id != 0);

        Self(id)
    }
}

impl From<u32> for ChainId {
    fn from(id: u32) -> Self {
        Self::new(id)
    }
}

impl fmt::Debug for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChainId({})", self.0)
    }
}

/// Ethereum Virtual Machine Chain
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Chain {
    pub chain_id: ChainId,
    /// RPC url of the chain which will be used for Blockchain interactions.
    ///
    /// # Ethereum Virtual Machine
    /// Check out the metadata for all EVM-chains: <https://github.com/ethereum-lists/chains>
    /// Or in json: <https://chainid.network/chains.json>
    pub rpc: ApiUrl,
    /// The OUTPACE contract address on this Chain
    pub outpace: Address,
    /// The Sweeper contract address on this Chain
    #[deprecated = "we no longer need the sweeper contract for deposits"]
    pub sweeper: Address,
}

/// Context of [`TokenInfo`] & [`Chain`] information for given [`Channel`] or [`Campaign`].
///
/// The additional context of [`Channel`] is known after checking if the `Channel` token's
/// `Chain` & `Address` are whitelisted in the configuration.
#[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Hash, Clone)]
pub struct ChainOf<T = ()> {
    pub context: T,
    pub chain: Chain,
    pub token: TokenInfo,
}

impl<T> ChainOf<T> {
    pub fn with<C>(self, context: C) -> ChainOf<C> {
        ChainOf {
            context,
            chain: self.chain,
            token: self.token,
        }
    }
}

impl ChainOf<()> {
    pub fn new(chain: Chain, token: TokenInfo) -> ChainOf<()> {
        ChainOf {
            context: (),
            chain,
            token,
        }
    }

    pub fn with_channel(self, channel: Channel) -> ChainOf<Channel> {
        ChainOf {
            context: channel,
            chain: self.chain,
            token: self.token,
        }
    }

    pub fn with_campaign(self, campaign: Campaign) -> ChainOf<Campaign> {
        ChainOf {
            context: campaign,
            chain: self.chain,
            token: self.token,
        }
    }
}

impl ChainOf<Campaign> {
    /// Get a [`Channel`] with [`Chain`] & [`TokenInfo`] context from
    /// the given [`Campaign`].
    pub fn of_channel(&self) -> ChainOf<Channel> {
        ChainOf {
            context: self.context.channel,
            token: self.token.clone(),
            chain: self.chain.clone(),
        }
    }
}
