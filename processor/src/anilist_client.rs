use crate::sleeper::Sleeper;
use anyhow::{anyhow, Result};
use common::time::{StdDuration, MINUTE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// Constructor for our global http client
///
/// We set our user agent so that upstream can contact us if necessary.
pub fn reqwest_client(user_agent: &str) -> Client {
    Client::builder().user_agent(user_agent).build().unwrap()
}

/// A client for the anilist graphql API
///
/// This client has several nice properties:
///
/// - It guarantees that no two requests are performed at the same time
/// - It enforces a timeout between requests
/// - It automatically handles rate-limit errors by sleeping and retrying
pub struct AnilistClient<'a> {
    client: &'a Client,
    inner: Mutex<Inner>,
}

struct Inner {
    sleeper: Sleeper,
    retry_after: Option<u64>,
}

#[derive(Deserialize, Debug)]
struct Response<T> {
    data: Option<T>,
    errors: Option<Vec<Error>>,
}

#[derive(Serialize, Debug)]
struct Body<'a, T> {
    query: &'a str,
    variables: &'a T,
}

#[derive(Deserialize, Debug)]
struct Error {
    message: String,
    status: i32,
}

#[derive(Deserialize, Debug)]
pub struct PageInfo {
    pub total: i32,
    pub per_page: i32,
    pub current_page: i32,
    pub last_page: i32,
    pub has_next_page: bool,
}

impl<'a> AnilistClient<'a> {
    pub fn new(client: &'a Client) -> Self {
        Self {
            client,
            inner: Mutex::new(Inner {
                sleeper: Sleeper::new(),
                retry_after: None,
            }),
        }
    }

    pub async fn request<V: Serialize, T: for<'b> Deserialize<'b>>(
        &self,
        query: &str,
        variables: &V,
    ) -> T {
        let mut inner = self.inner.lock().await;
        // If everything were working properly, this one second timeout should ensure that
        // we never go over the 90 requests/minute limit imposed by the anilist API.
        // However: https://github.com/AniList/ApiV2-GraphQL-Docs/issues/103
        inner.sleeper.sleep(StdDuration::from_secs(1)).await;
        loop {
            match self.request_(&mut inner, query, variables).await {
                Ok(d) => return d,
                Err(e) => {
                    log::error!("could perform request: {:#}", e);
                    let delay = match inner.retry_after.take() {
                        Some(retry_after) => StdDuration::from_secs(retry_after),
                        _ => {
                            // Some error has occurred that is not related to rate
                            // limiting.
                            MINUTE
                        }
                    };
                    log::info!("sleeping for {} seconds", delay.as_secs());
                    tokio::time::delay_for(delay).await;
                    // Mark the start of the next try in the sleeper so that the next user
                    // gets delayed appropriately.
                    inner.sleeper.set_now();
                }
            }
        }
    }

    async fn request_<V: Serialize, T: for<'b> Deserialize<'b>>(
        &self,
        inner: &mut Inner,
        query: &str,
        variables: &V,
    ) -> Result<T> {
        let body = Body { query, variables };
        let response = self
            .client
            .post("https://graphql.anilist.co")
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await?;
        if let Some(limit) = response.headers().get("Retry-After") {
            if let Ok(limit) = limit.to_str() {
                if let Ok(num) = limit.parse::<u64>() {
                    inner.retry_after = Some(num + 10);
                    return Err(anyhow!("Retry-After header is set: {}", num));
                }
            }
            return Err(anyhow!(
                "Retry-After header is set but the value is invalid"
            ));
        }
        let text = response.text().await?;
        let response: Response<T> = match serde_json::from_str(&text) {
            Ok(p) => p,
            Err(_) => return Err(anyhow!("cannot parse response {}", text)),
        };
        if let Some(d) = response.data {
            return Ok(d);
        }
        Err(anyhow!(
            "response data is null, errors: {:?}",
            response.errors
        ))
    }
}
