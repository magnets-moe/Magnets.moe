use reqwest::Client;

/// Constructor for our global http client
///
/// We set our user agent so that upstream can contact us if necessary.
pub fn reqwest_client(user_agent: &str) -> Client {
    Client::builder().user_agent(user_agent).build().unwrap()
}
