
pub struct Storage {
    pub channel: None,
    pub event_aggregates: None,
    pub validator_messages: None
}

impl Storage {
    pub fn new(db_pool: DbPool) -> Self {
        Self { db_pool }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
