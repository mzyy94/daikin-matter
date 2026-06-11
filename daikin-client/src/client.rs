//! HTTP client implementations for Daikin devices.

use async_lock::RwLock;
use dsiot::protocol::{DaikinInfo, DaikinRequest, DaikinResponse, DaikinStatus};
use serde_json::json;
use serde_json::value::Value;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Trait for HTTP clients that can communicate with Daikin devices.
#[allow(async_fn_in_trait)]
pub trait HttpClient {
    async fn send_request(&self, url: &str, payload: Value) -> anyhow::Result<Value>;
}

/// Reqwest-based HTTP client for Daikin devices.
#[derive(Clone, Debug)]
pub struct ReqwestClient {
    client: reqwest::Client,
}

impl ReqwestClient {
    /// Create a new ReqwestClient with default settings.
    pub fn try_new() -> Result<Self, reqwest::Error> {
        Ok(ReqwestClient {
            client: reqwest::Client::builder()
                .http1_title_case_headers()
                .timeout(Duration::new(5, 0))
                .build()?,
        })
    }
}

impl HttpClient for ReqwestClient {
    async fn send_request(&self, url: &str, payload: Value) -> anyhow::Result<Value> {
        let response = self.client.post(url).json(&payload).send().await?;
        let body = response.json().await?;
        Ok(body)
    }
}

struct Cache {
    last_updated: Instant,
    data: Option<DaikinStatus>,
}

impl Cache {
    fn new() -> Self {
        Cache {
            last_updated: Instant::now(),
            data: None,
        }
    }

    fn update(&mut self, value: DaikinStatus) {
        self.last_updated = Instant::now();
        self.data = Some(value);
    }

    fn get(&self) -> Option<DaikinStatus> {
        if self.last_updated.elapsed().as_millis() < 5000 {
            self.data.clone()
        } else {
            None
        }
    }
}

/// Daikin device client.
#[derive(Clone)]
pub struct Daikin<H: HttpClient> {
    endpoint: String,
    cache: Arc<RwLock<Cache>>,
    client: Arc<H>,
}

impl<H: HttpClient> std::fmt::Debug for Daikin<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Daikin {{ endpoint: {} }}", self.endpoint)
    }
}

impl<H: HttpClient> Daikin<H> {
    /// Create a new Daikin client for the device at the given IP address.
    pub fn new(ip_addr: Ipv4Addr, client: H) -> Daikin<H> {
        Daikin {
            endpoint: format!("http://{ip_addr}/dsiot/multireq"),
            cache: Arc::new(RwLock::new(Cache::new())),
            client: Arc::new(client),
        }
    }

    /// Get the current device status.
    pub async fn get_status(&self) -> anyhow::Result<DaikinStatus> {
        if let Some(status) = self.cache.read().await.get() {
            return Ok(status);
        }
        let payload = json!({"requests": [
            {
                "op": 2,
                "to": "/dsiot/edge/adr_0100.dgc_status?filter=pv,md"
            },
            {
                "op": 2,
                "to": "/dsiot/edge/adr_0200.dgc_status?filter=pv,md"
            }
        ]});

        let body = self.client.send_request(&self.endpoint, payload).await?;
        let status: DaikinStatus = serde_json::from_value::<DaikinResponse>(body)?.into();

        let mut cache = self.cache.write().await;
        cache.update(status.clone());

        Ok(status)
    }

    /// Get device information.
    pub async fn get_info(&self) -> anyhow::Result<DaikinInfo> {
        let payload = json!({"requests": [
            {
                "op": 2,
                "to": "/dsiot/edge.adp_i"
            },
            {
                "op": 2,
                "to": "/dsiot/edge.adp_d"
            },
            {
                "op": 2,
                "to": "/dsiot/edge.adp_r"
            }
        ]});

        let body = self.client.send_request(&self.endpoint, payload).await?;
        let info: DaikinInfo = serde_json::from_value::<DaikinResponse>(body)?.into();

        Ok(info)
    }

    /// Update device status.
    pub async fn update(&self, status: DaikinStatus) -> anyhow::Result<()> {
        let payload = serde_json::to_value(DaikinRequest::from(status.clone()))?;
        let body = self.client.send_request(&self.endpoint, payload).await?;
        // Reject the write if the device returned an error status code.
        serde_json::from_value::<DaikinResponse>(body)?;
        self.cache.write().await.update(status);
        Ok(())
    }
}
