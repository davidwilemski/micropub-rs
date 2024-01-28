use std::sync::Arc;

use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::SqliteConnection;
use log::error;

use crate::errors::DBError;

pub trait WithDB {
    fn dbpool(&self) -> &Pool<ConnectionManager<SqliteConnection>>;

    fn handle_errors(&self, e: diesel::result::Error) -> DBError {
        error!("{:?}", e);
        match e {
            diesel::result::Error::NotFound => DBError::not_found(),
            _ => DBError::new(),
        }
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn dbconn(&self) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, DBError> {
        self.dbpool().get().map_err(|e| {
            error!("{:?}", e);
            DBError::new()
        })
    }

    #[tracing::instrument(level = "debug", skip(self, f))]
    fn run_txn<T, F>(&self, f: F) -> Result<T, DBError>
    where
        F: FnOnce(
            &mut PooledConnection<ConnectionManager<SqliteConnection>>,
        ) -> Result<T, diesel::result::Error>,
    {
        let mut conn = self.dbconn()?;
        conn.transaction(|c| f(c))
            .map_err(|e| self.handle_errors(e))
    }
}

pub struct MicropubDB {
    dbpool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
}

impl MicropubDB {
    pub fn new(dbpool: Arc<Pool<ConnectionManager<SqliteConnection>>>) -> Self {
        Self { dbpool }
    }
}

impl WithDB for MicropubDB {
    fn dbpool(&self) -> &Pool<ConnectionManager<SqliteConnection>> {
        &self.dbpool
    }
}
