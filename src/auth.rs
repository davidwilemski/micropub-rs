use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct TokenValidateResponse {
    pub me: String,
    pub client_id: String,
    pub issued_at: i64,
    pub scope: String,
    pub nonce: i64,
}

impl TokenValidateResponse {
    pub fn scopes(&self) -> Vec<&str> {
        if self.scope.is_empty() {
            vec![]
        } else {
            self.scope.split_whitespace().collect()
        }
    }
}