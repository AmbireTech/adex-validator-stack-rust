use primitives::Channel;
use web3::ethabi::Token;

pub(super) trait EthereumChannel {
    fn tokenize(&self) -> Token;
}

impl EthereumChannel for Channel {
    fn tokenize(&self) -> Token {
        let tokens = vec![
            Token::Address(self.leader.as_bytes().into()),
            Token::Address(self.follower.as_bytes().into()),
            Token::Address(self.guardian.as_bytes().into()),
            Token::Address(self.token.as_bytes().into()),
            Token::FixedBytes(self.nonce.to_bytes().to_vec()),
        ];

        Token::Tuple(tokens)
    }
}
