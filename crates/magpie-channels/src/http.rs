//! The one HTTP operation channels need: POST JSON, read status + body.

use std::time::Duration;

use serde_json::Value;

use crate::{BoxFuture, ChannelError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

pub trait HttpClient: Send + Sync {
    fn post_json<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(&'a str, String)],
        body: &'a Value,
    ) -> BoxFuture<'a, Result<HttpResponse, ChannelError>>;
}

#[derive(Debug, Clone)]
pub struct ReqwestHttp {
    client: reqwest::Client,
}

impl ReqwestHttp {
    pub fn new(timeout: Duration) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .user_agent(concat!("magpie/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("static reqwest config is valid");
        Self { client }
    }
}

impl HttpClient for ReqwestHttp {
    fn post_json<'a>(
        &'a self,
        url: &'a str,
        headers: &'a [(&'a str, String)],
        body: &'a Value,
    ) -> BoxFuture<'a, Result<HttpResponse, ChannelError>> {
        Box::pin(async move {
            let mut req = self.client.post(url).json(body);
            for (k, v) in headers {
                req = req.header(*k, v);
            }
            // `without_url`: Telegram puts the bot token in the URL path, and
            // reqwest errors print the URL. Never let it reach logs or the UI.
            let res = req
                .send()
                .await
                .map_err(|e| ChannelError::Transient(e.without_url().to_string()))?;
            let status = res.status().as_u16();
            let body = res
                .text()
                .await
                .map_err(|e| ChannelError::Transient(e.without_url().to_string()))?;
            Ok(HttpResponse { status, body })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real reqwest against a closed loopback port: the error must not echo the
    /// URL, because Telegram URLs contain the bot token.
    #[tokio::test]
    async fn transport_errors_never_contain_the_url() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port(); // listener dropped here: the port is now closed
        let http = ReqwestHttp::new(Duration::from_secs(5));
        let url = format!("http://127.0.0.1:{port}/bot123:SECRET_TOKEN/sendMessage");
        let err = http.post_json(&url, &[], &Value::Null).await.unwrap_err();
        let text = err.to_string();
        assert!(matches!(err, ChannelError::Transient(_)), "{text}");
        assert!(!text.contains("SECRET_TOKEN"), "token leaked: {text}");
    }
}

#[cfg(test)]
pub(crate) mod fake {
    use std::sync::Mutex;

    use super::{HttpClient, HttpResponse};
    use crate::{BoxFuture, ChannelError};
    use serde_json::Value;

    #[derive(Debug, Clone, PartialEq)]
    pub struct Recorded {
        pub url: String,
        pub headers: Vec<(String, String)>,
        pub body: Value,
    }

    /// Records every request and answers with a scripted response.
    #[derive(Debug)]
    pub struct FakeHttp {
        pub requests: Mutex<Vec<Recorded>>,
        reply: Result<HttpResponse, ChannelError>,
    }

    impl FakeHttp {
        pub fn replying(status: u16, body: &str) -> Self {
            Self {
                requests: Mutex::default(),
                reply: Ok(HttpResponse {
                    status,
                    body: body.to_owned(),
                }),
            }
        }

        pub fn failing(err: ChannelError) -> Self {
            Self {
                requests: Mutex::default(),
                reply: Err(err),
            }
        }

        pub fn only_request(&self) -> Recorded {
            let reqs = self.requests.lock().unwrap();
            assert_eq!(reqs.len(), 1, "expected exactly one request");
            reqs[0].clone()
        }
    }

    impl HttpClient for FakeHttp {
        fn post_json<'a>(
            &'a self,
            url: &'a str,
            headers: &'a [(&'a str, String)],
            body: &'a Value,
        ) -> BoxFuture<'a, Result<HttpResponse, ChannelError>> {
            self.requests.lock().unwrap().push(Recorded {
                url: url.to_owned(),
                headers: headers
                    .iter()
                    .map(|(k, v)| ((*k).to_owned(), v.clone()))
                    .collect(),
                body: body.clone(),
            });
            let reply = self.reply.clone();
            Box::pin(async move { reply })
        }
    }
}
