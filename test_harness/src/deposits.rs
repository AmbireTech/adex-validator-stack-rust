use primitives::{config::TokenInfo, Address, BigNum, Channel};

#[derive(Debug, Clone)]
pub struct Deposit {
    pub channel: Channel,
    pub token: TokenInfo,
    pub address: Address,
    /// In native token precision
    pub outpace_amount: BigNum,
    /// In native token precision
    pub counterfactual_amount: BigNum,
}

impl PartialEq<primitives::Deposit<BigNum>> for Deposit {
    fn eq(&self, other: &primitives::Deposit<BigNum>) -> bool {
        let total = &self.outpace_amount + &self.counterfactual_amount;

        self.counterfactual_amount == other.still_on_create2 && total == other.total
    }
}
