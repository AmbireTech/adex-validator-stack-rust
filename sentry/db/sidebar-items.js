initSidebarItems({"fn":[["get_channel_by_id",""],["insert_channel","Used to insert/get Channel when creating a Campaign If channel already exists it will return it instead. This call should never trigger a `SqlState::UNIQUE_VIOLATION`"],["list_channels","Lists the `Channel`s in `ASC` order."],["postgres_connection",""],["redis_connection",""],["setup_migrations","Sets the migrations using the `POSTGRES_*` environment variables"]],"mod":[["accounting",""],["analytics",""],["campaign",""],["redis_pool",""],["spendable",""],["tests_postgres",""],["validator_message",""]],"struct":[["PostgresConfig","Connection configuration."],["RedisError","Represents a redis error.  For the most part you should be using the Error trait to interact with this rather than the actual struct."],["TotalCount",""]],"type":[["DbPool",""],["PoolError","Type alias for using [`deadpool::managed::PoolError`] with [`tokio_postgres`]."]]});