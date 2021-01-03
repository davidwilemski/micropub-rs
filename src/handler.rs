use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::SqliteConnection;

use crate::errors::DBError;

pub trait WithDB {
    fn dbpool(&self) -> &Pool<ConnectionManager<SqliteConnection>>;

    fn dbconn(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, DBError> {
        self.dbpool().get().map_err(|e| {
            println!("{:?}", e);
            DBError
        })
    }

    fn run_txn<T, F>(&self, f: F) -> Result<T, DBError>
    where
        F: FnOnce(
            &PooledConnection<ConnectionManager<SqliteConnection>>,
        ) -> Result<T, diesel::result::Error>,
    {
        let conn = self.dbconn()?;
        conn.transaction(|| f(&conn)).map_err(|e| {
            println!("{:?}", e);
            DBError
        })
    }
}

pub struct MicropubDB {
    dbpool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
}

impl MicropubDB {
    pub fn new(dbpool: Arc<Pool<ConnectionManager<SqliteConnection>>>) -> Self {
        Self { dbpool: dbpool }
    }
}

impl WithDB for MicropubDB {
    fn dbpool(&self) -> &Pool<ConnectionManager<SqliteConnection>> {
        &self.dbpool
    }
}
