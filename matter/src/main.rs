#[macro_use]
extern crate log;

mod bridge;
mod bridged_info;
mod device;
mod fan_control;
mod humidity;
mod identify;
mod mdns;
mod onoff;
mod power;
mod power_topology;
mod runtime;
mod thermostat;
mod wifi_diag;

use core::pin::pin;
use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use daikin_client::{Daikin, ReqwestClient, discovery};
use dsiot::DaikinInfo;
use futures_lite::StreamExt;

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// IPv4 address of Daikin AC
    #[arg(
        value_name = "ip_address",
        env = "DAIKIN_IP_ADDRS",
        value_delimiter = ' '
    )]
    ip_addrs: Vec<Ipv4Addr>,

    /// Discovery timeout in milliseconds
    #[arg(long, env = "DAIKIN_TIMEOUT", default_value = "3000")]
    timeout: u64,

    /// Expected number of devices to discover
    #[arg(
        long,
        env = "DAIKIN_COUNT",
        default_value = "128",
        hide_default_value = true
    )]
    count: usize,

    /// Directory to store persistent data (pairing, fabrics, etc.)
    #[arg(long, env = "DAIKIN_DATA_DIR", value_name = "DIR")]
    data_dir: Option<PathBuf>,

    /// File containing the Gen5 local API key for HTTPS adapters
    #[arg(long, env = "DAIKIN_LOCAL_API_KEY_FILE", value_name = "PATH")]
    local_api_key_file: Option<PathBuf>,
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("daikin-matter")
}

fn read_local_api_key(path: &Path) -> anyhow::Result<String> {
    let contents = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read local API key file {}", path.display()))?;
    let local_api_key = contents.trim().to_string();
    if local_api_key.is_empty() {
        anyhow::bail!("local API key is empty: {}", path.display());
    }
    Ok(local_api_key)
}

async fn connect_http(ip_addr: Ipv4Addr) -> anyhow::Result<(Daikin<ReqwestClient>, DaikinInfo)> {
    let dk = Daikin::new(ip_addr, ReqwestClient::try_new()?);
    let info = dk.get_info().await?;
    info!(
        "Device: {} (MAC: {}, EDID: {})",
        info.name, info.mac, info.edid
    );
    let status = dk.get_status().await?;
    debug!("Status: {:?}", status);
    Ok((dk, info))
}

async fn connect_https(
    ip_addr: Ipv4Addr,
    local_api_key: &str,
) -> anyhow::Result<(Daikin<ReqwestClient>, DaikinInfo)> {
    let dk = Daikin::new_https(
        ip_addr,
        ReqwestClient::try_new_with_local_api_key(local_api_key)?,
    );
    let info = dk.get_info().await?;
    info!(
        "Device: {} (MAC: {}, EDID: {}) via HTTPS",
        info.name, info.mac, info.edid
    );
    let status = dk.get_status().await?;
    debug!("Status: {:?}", status);
    Ok((dk, info))
}

async fn connect_explicit_ip(
    ip_addr: Ipv4Addr,
    local_api_key: Option<&str>,
) -> anyhow::Result<(Daikin<ReqwestClient>, DaikinInfo)> {
    if let Some(local_api_key) = local_api_key {
        match connect_https(ip_addr, local_api_key).await {
            Ok(connection) => return Ok(connection),
            Err(error) => {
                warn!("HTTPS connection to {ip_addr} failed; falling back to HTTP: {error:#}");
            }
        }
    }

    connect_http(ip_addr).await
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("daikin_matter=debug,rs_matter=info"),
    )
    .init();

    let cli = Cli::parse();
    let local_api_key = match cli.local_api_key_file.as_deref() {
        Some(path) => Some(read_local_api_key(path)?),
        None => std::env::var("DAIKIN_LOCAL_API_KEY")
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    };

    let rt = tokio::runtime::Runtime::new()?;
    let connections: Vec<(Daikin<ReqwestClient>, DaikinInfo)> = rt.block_on(async {
        let mut conns = Vec::new();
        if cli.ip_addrs.is_empty() {
            info!("No IP addresses specified, discovering devices...");
            let timeout = Duration::from_millis(cli.timeout);
            let stream = discovery(timeout).await;
            let mut stream = pin!(stream);
            while let Some(result) = stream.next().await {
                match result {
                    Ok((dk, _udp_info)) => {
                        let info = dk.get_info().await?;
                        debug!("Status: {:?}", dk.get_status().await?);
                        conns.push((dk, info));
                        if conns.len() >= cli.count {
                            break;
                        }
                    }
                    Err(e) => warn!("Discovery error: {e}"),
                }
            }
        } else {
            for ip in &cli.ip_addrs {
                conns.push(connect_explicit_ip(*ip, local_api_key.as_deref()).await?);
            }
        }
        if conns.is_empty() {
            anyhow::bail!("No devices found");
        }
        if cli.count != 128 && conns.len() < cli.count {
            anyhow::bail!(
                "Found only {} devices, but requested {}",
                conns.len(),
                cli.count
            );
        }
        anyhow::Ok(conns)
    })?;

    let rt_handle = rt.handle().clone();
    let data_dir = cli.data_dir.unwrap_or_else(default_data_dir);
    info!("Data directory: {}", data_dir.display());

    let thread = std::thread::Builder::new()
        .stack_size(1024 * 1024)
        .spawn(move || runtime::run_matter(connections, rt_handle, data_dir))
        .unwrap();

    thread.join().unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn read_local_api_key_trims_file_contents() {
        let path = std::env::temp_dir().join(format!(
            "daikin-matter-local-api-key-{}-trim",
            std::process::id()
        ));
        fs::write(&path, "  secret-value\n").unwrap();

        let key = read_local_api_key(&path).unwrap();

        assert_eq!(key, "secret-value");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn read_local_api_key_rejects_empty_file() {
        let path = std::env::temp_dir().join(format!(
            "daikin-matter-local-api-key-{}-empty",
            std::process::id()
        ));
        fs::write(&path, "\n\t ").unwrap();

        let error = read_local_api_key(&path).unwrap_err();

        assert!(error.to_string().contains("local API key is empty"));
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn cli_accepts_local_api_key_file_with_explicit_ips() {
        let cli = Cli::try_parse_from([
            "daikin-matter",
            "--local-api-key-file",
            "/var/lib/daikin-matter/local_api_key",
            "192.168.1.150",
            "192.168.1.151",
            "192.168.1.152",
            "192.168.1.153",
        ])
        .unwrap();

        assert_eq!(
            cli.local_api_key_file.as_deref(),
            Some(Path::new("/var/lib/daikin-matter/local_api_key"))
        );
        assert_eq!(cli.ip_addrs.len(), 4);
    }
}
