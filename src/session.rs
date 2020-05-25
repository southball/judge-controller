use crate::api::*;
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::clone::Clone;
use std::collections::HashMap;
use url::Url;

#[derive(Clone)]
pub struct Session {
    pub base_url: Url,
    pub expiry: DateTime<Utc>,
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JWTTokenPair {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JWTClaims {
    pub exp: i64,
}

impl Session {
    pub fn new(base_url: &str) -> Session {
        Session {
            base_url: Url::parse(base_url).unwrap(),
            expiry: Utc.timestamp(0, 0),
            access_token: String::new(),
            refresh_token: String::new(),
        }
    }

    pub fn resolve(&self, url_fragment: Vec<&str>) -> Url {
        url_fragment
            .iter()
            .fold(self.base_url.clone(), |url, fragment| {
                url.join(fragment).unwrap()
            })
    }

    pub fn resolve_single(&self, url_fragment: &str) -> Url {
        self.resolve(vec![url_fragment])
    }

    pub async fn init(&mut self, username: &str, password: &str) {
        let client = reqwest::Client::new();
        let body = {
            let mut map = HashMap::new();
            map.insert("username", username);
            map.insert("password", password);
            map
        };
        let response = client
            .post(self.resolve_single("auth/login"))
            .json(&body)
            .send()
            .await
            .unwrap()
            .json::<ApiSuccess<JWTTokenPair>>()
            .await
            .unwrap();

        self.access_token = String::from(response.data.access_token);
        self.refresh_token = String::from(response.data.refresh_token);

        self.recalc_expiry();
    }

    /// Recompute expiry time from `self.access_token`.
    pub fn recalc_expiry(&mut self) {
        let token_message =
            jsonwebtoken::dangerous_unsafe_decode::<JWTClaims>(&self.access_token).unwrap();
        self.expiry = Utc.timestamp(token_message.claims.exp, 0);
    }

    pub async fn refresh(&mut self) {
        let client = reqwest::Client::new();
        let body = {
            let mut map = HashMap::new();
            map.insert("refresh_token", &self.refresh_token);
            map
        };
        let response = client
            .post(self.resolve_single("auth/refresh"))
            .json(&body)
            .send()
            .await
            .unwrap()
            .json::<ApiSuccess<JWTTokenPair>>()
            .await
            .unwrap();

        self.access_token = String::from(response.data.access_token);
        self.recalc_expiry();
    }

    pub async fn get_access_token(&mut self) -> &str {
        if Utc::now() + Duration::minutes(5) > self.expiry {
            self.refresh().await;
        }

        &self.access_token
    }
}
