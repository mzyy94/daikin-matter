//! Device discovery for Daikin air conditioners.

use crate::client::{Daikin, ReqwestClient};
use async_stream::try_stream;
use dsiot::protocol::DaikinInfo;
use futures::prelude::*;
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::str;
use std::time::Duration;
use tokio::net::UdpSocket;

const FALLBACK: (IpAddr, IpAddr) = (
    IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0)),
    IpAddr::V4(Ipv4Addr::new(255, 255, 255, 255)),
);

fn get_ipaddr() -> (IpAddr, IpAddr) {
    let network_interfaces = match NetworkInterface::show() {
        Ok(i) => i,
        Err(_) => return FALLBACK,
    };

    let nis: Vec<&NetworkInterface> = network_interfaces
        .iter()
        .filter(|ni| {
            ni.mac_addr != Some("00:00:00:00:00:00".into())
                && ni.mac_addr.is_some()
                && ni.addr.iter().filter(|a| a.ip().is_ipv4()).count() > 0
        })
        .collect();

    if nis.len() != 1 {
        return FALLBACK;
    }

    // Safe: filter above guarantees at least one IPv4 address exists
    let Some(ipv4_addr) = nis[0].addr.iter().find(|a| a.ip().is_ipv4()) else {
        return FALLBACK;
    };
    (ipv4_addr.ip(), ipv4_addr.broadcast().unwrap_or(FALLBACK.1))
}

/// Discover Daikin devices on the local network.
///
/// Returns a stream of discovered devices with their information.
pub async fn discovery(
    timeout: Duration,
) -> impl Stream<Item = anyhow::Result<(Daikin<ReqwestClient>, DaikinInfo)>> {
    let (srcip, dstip) = get_ipaddr();
    let src_addr = format!("{srcip}:30000");
    let dst_addr = format!("{dstip}:30050");

    debug!("discovering daikin device from {src_addr} to {dst_addr}");

    try_stream! {
        let socket = UdpSocket::bind(src_addr).await?;
        socket.set_broadcast(true)?;
        let payload = "DAIKIN_UDP/common/basic_info";

        socket.send_to(payload.as_bytes(), dst_addr).await?;

        loop {
            let mut buf = [0; 2048];
            let Ok(res) = tokio::time::timeout(timeout, socket.recv_from(&mut buf)).await else {
                debug!("Discovery timed out after {timeout:?}");
                break;
            };
            let (text, src_addr) = match res? {
                (2048, _) => {
                    warn!("UDP buffer too small");
                    continue;
                }
                (buf_size, SocketAddr::V4(src_addr)) => {
                    (str::from_utf8(&buf[..buf_size])?, src_addr)
                }
                _ => {
                    continue;
                }
            };


            let daikin = Daikin::new(*src_addr.ip(), ReqwestClient::try_new()?);
            let Ok(info) = serde_qs::from_str::<DaikinInfo>(&text.replace(',', "&")) else {
                warn!("Failed to parse device info from {}", src_addr.ip());
                continue;
            };

            info!(
                "found daikin device at {}: {}",
                src_addr.ip(),
                info.name
            );

            yield (daikin, info);
        }
    }
}
