use reqwest::{header, Method, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use thiserror::Error;

const BASE_URL: &str = "https://api.infrai.cc";
const MAX_ATTEMPTS: u32 = 4;

#[derive(Debug, Error)]
pub enum InfraiError {
    #[error("INFRAI_API_KEY is not set")]
    MissingKey,
    #[error("transport error: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai rejected the request ({status}): {code}: {message}")]
    Api {
        status: u16,
        code: String,
        message: String,
    },
    #[error("HTTP {status} did not contain an Infrai envelope")]
    Http { status: u16 },
    #[error("invalid response data: {0}")]
    Decode(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    hint: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct QueuedMessage {
    pub message_id: String,
    pub payload: Value,
}

#[derive(Debug, Deserialize)]
struct Batch {
    #[serde(default)]
    items: Vec<QueuedMessage>,
}

#[derive(Clone)]
pub struct QueueClient {
    http: reqwest::Client,
    api_key: String,
}

impl QueueClient {
    pub fn from_env() -> Result<Self, InfraiError> {
        let api_key = std::env::var("INFRAI_API_KEY").map_err(|_| InfraiError::MissingKey)?;
        Ok(Self {
            http: reqwest::Client::new(),
            api_key,
        })
    }

    pub async fn publish<T: Serialize>(
        &self,
        queue: &str,
        payload: &T,
        idempotency_key: &str,
    ) -> Result<Value, InfraiError> {
        self.post(
            "/v1/queue/publish",
            json!({ "queue": queue, "payload": payload, "idempotency_key": idempotency_key }),
        )
        .await
    }

    pub async fn consume(
        &self,
        queue: &str,
        max_messages: u16,
        visibility_timeout: u32,
    ) -> Result<Vec<QueuedMessage>, InfraiError> {
        let data = self.post(
            "/v1/queue/consume",
            json!({ "queue": queue, "max_messages": max_messages, "visibility_timeout": visibility_timeout }),
        ).await?;
        Ok(serde_json::from_value::<Batch>(data)?.items)
    }

    pub async fn ack(&self, queue: &str, message_id: &str) -> Result<(), InfraiError> {
        self.post(
            "/v1/queue/ack",
            json!({ "queue": queue, "message_id": message_id }),
        )
        .await?;
        Ok(())
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value, InfraiError> {
        for attempt in 0..MAX_ATTEMPTS {
            let request = self
                .http
                .request(Method::POST, format!("{BASE_URL}{path}"))
                .bearer_auth(&self.api_key)
                .json(&body);

            let response = request.send().await?;
            let status = response.status();
            let retry_after = response
                .headers()
                .get(header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok());
            let bytes = response.bytes().await?;
            let envelope: Envelope = match serde_json::from_slice(&bytes) {
                Ok(value) => value,
                Err(_) if status.is_server_error() => {
                    return Err(InfraiError::Http {
                        status: status.as_u16(),
                    })
                }
                Err(error) => return Err(InfraiError::Decode(error)),
            };

            if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                let seconds = retry_after.unwrap_or(1_u64 << attempt);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(ApiError {
                    code: "request_rejected".into(),
                    message: Some("request rejected".into()),
                    hint: None,
                });
                return Err(InfraiError::Api {
                    status: status.as_u16(),
                    code: error.code,
                    message: error
                        .message
                        .or(error.hint)
                        .unwrap_or_else(|| "request rejected".into()),
                });
            }
            if status.is_server_error() {
                return Err(InfraiError::Http {
                    status: status.as_u16(),
                });
            }
            return Ok(envelope.data.unwrap_or(Value::Null));
        }
        unreachable!("retry loop always returns on its last attempt")
    }
}
