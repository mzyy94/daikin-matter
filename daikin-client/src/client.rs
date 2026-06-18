//! HTTP client implementations for Daikin devices.

use async_lock::RwLock;
use dsiot::protocol::{DaikinInfo, DaikinRequest, DaikinResponse, DaikinStatus};
use reqwest::header::{AUTHORIZATION, HeaderValue};
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
#[derive(Clone)]
pub struct ReqwestClient {
    client: reqwest::Client,
    local_api_key: Option<HeaderValue>,
}

impl ReqwestClient {
    /// Create a new ReqwestClient with default settings.
    pub fn try_new() -> Result<Self, reqwest::Error> {
        Self::try_new_with_options(false, None)
    }

    fn try_new_with_options(
        accept_invalid_certs: bool,
        local_api_key: Option<HeaderValue>,
    ) -> Result<Self, reqwest::Error> {
        let client = reqwest::Client::builder()
            .http1_title_case_headers()
            .danger_accept_invalid_certs(accept_invalid_certs)
            .timeout(Duration::new(5, 0))
            .build()?;

        Ok(Self {
            client,
            local_api_key,
        })
    }

    /// Create a new ReqwestClient that sends the Gen5 local API key.
    pub fn try_new_with_local_api_key(local_api_key: impl AsRef<str>) -> anyhow::Result<Self> {
        let local_api_key = local_api_key.as_ref().trim();
        if local_api_key.is_empty() {
            anyhow::bail!("local API key is empty");
        }
        let mut header = HeaderValue::from_str(local_api_key)?;
        header.set_sensitive(true);

        Ok(Self::try_new_with_options(true, Some(header))?)
    }
}

impl std::fmt::Debug for ReqwestClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReqwestClient")
            .field("local_api_key_configured", &self.local_api_key.is_some())
            .finish_non_exhaustive()
    }
}

impl HttpClient for ReqwestClient {
    async fn send_request(&self, url: &str, payload: Value) -> anyhow::Result<Value> {
        let mut request = self.client.post(url).json(&payload);
        if let Some(local_api_key) = &self.local_api_key {
            request = request.header(AUTHORIZATION, local_api_key.clone());
        }
        let response = request.send().await?;
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
        Self::with_scheme("http", ip_addr, client)
    }

    /// Create a new HTTPS Daikin client for the device at the given IP address.
    pub fn new_https(ip_addr: Ipv4Addr, client: H) -> Daikin<H> {
        Self::with_scheme("https", ip_addr, client)
    }

    fn with_scheme(scheme: &str, ip_addr: Ipv4Addr, client: H) -> Daikin<H> {
        Daikin {
            endpoint: format!("{scheme}://{ip_addr}/dsiot/multireq"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, TcpListener};
    use std::thread;

    #[test]
    fn new_https_uses_https_multireq_endpoint() {
        let daikin = Daikin::new_https(
            Ipv4Addr::new(192, 168, 1, 152),
            ReqwestClient::try_new().unwrap(),
        );

        assert_eq!(daikin.endpoint, "https://192.168.1.152/dsiot/multireq");
    }

    #[tokio::test]
    async fn local_api_key_is_sent_as_authorization_header() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/dsiot/multireq", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            loop {
                let read = stream.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|window| window == b"\r\n\r\n") {
                    break;
                }
            }
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
                )
                .unwrap();
            String::from_utf8(request).unwrap()
        });

        let client = ReqwestClient::try_new_with_local_api_key("test-local-key").unwrap();
        client
            .send_request(&url, json!({"requests": []}))
            .await
            .unwrap();

        let request = server.join().unwrap().to_ascii_lowercase();
        assert!(request.contains("\r\nauthorization: test-local-key\r\n"));
    }

    #[test]
    fn debug_output_does_not_include_local_api_key() {
        let client = ReqwestClient::try_new_with_local_api_key("test-local-key").unwrap();

        let debug = format!("{client:?}");

        assert!(debug.contains("local_api_key_configured: true"));
        assert!(!debug.contains("test-local-key"));
    }
}
