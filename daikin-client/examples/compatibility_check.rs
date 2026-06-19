use clap::Parser;
use daikin_client::{Daikin, ReqwestClient};
use dsiot::protocol::property::{Binary, Metadata};
use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::time;

#[derive(Parser)]
#[clap(
    author = "mzyy94",
    version = "v0.1.0",
    about = "Get current status from Daikin AC"
)]
struct Cli {
    /// IPv4 address of Daikin AC
    #[arg(value_name = "ip_address")]
    ip_addr: String,

    /// File containing the Gen5 local API key for HTTPS adapters
    #[arg(long, value_name = "PATH")]
    local_api_key_file: Option<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Transport {
    Http,
    Https,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let addr = cli.ip_addr.parse::<Ipv4Addr>()?;
    let local_api_key = cli
        .local_api_key_file
        .as_deref()
        .map(std::fs::read_to_string)
        .transpose()?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    run(addr, local_api_key.as_deref()).await
}

async fn fetch_udp_info(ip_addr: Ipv4Addr) -> anyhow::Result<HashMap<String, String>> {
    let socket = UdpSocket::bind("0.0.0.0:0").await?;
    socket
        .send_to(b"DAIKIN_UDP/common/basic_info", (ip_addr, 30050))
        .await?;
    let mut buf = [0u8; 2048];
    let (n, _) = time::timeout(Duration::from_secs(2), socket.recv_from(&mut buf)).await??;
    let text = std::str::from_utf8(&buf[..n])?;
    Ok(text
        .split(',')
        .filter_map(|kv| kv.split_once('='))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect())
}

fn section(title: &str) {
    println!("\n── {title} ──");
}

fn line(label: &str, value: impl std::fmt::Display) {
    println!("  {label:<22} {value}");
}

fn check_adapter_support(
    udp: &HashMap<String, String>,
    has_local_api_key: bool,
) -> Option<Transport> {
    let api_ver = udp.get("api_ver").map(String::as_str).unwrap_or("?");
    let adp_kind = udp.get("adp_kind").map(String::as_str).unwrap_or("?");

    line("Adapter kind", adp_kind);
    line("API version", api_ver);

    match (adp_kind, api_ver) {
        ("4", v) if v.starts_with("2_") => {
            println!("  ✅ HTTP DSIOT (supported)");
            Some(Transport::Http)
        }
        ("5", v) if v.starts_with("3_") => {
            if has_local_api_key {
                println!("  ✅ HTTPS DSIOT with local API key");
                Some(Transport::Https)
            } else {
                println!("  ❌ HTTPS DSIOT requires --local-api-key-file");
                None
            }
        }
        _ => {
            println!("  ⚠️  Unknown combination — please report at the issue tracker");
            None
        }
    }
}

async fn run(ip_addr: Ipv4Addr, local_api_key: Option<&str>) -> anyhow::Result<()> {
    println!("Daikin Compatibility Check");
    println!("  Target: {ip_addr}");

    section("Adapter");
    let transport = match fetch_udp_info(ip_addr).await {
        Ok(udp) => match check_adapter_support(&udp, local_api_key.is_some()) {
            Some(t) => t,
            None => return Ok(()),
        },
        Err(e) => {
            println!("  ⚠️  UDP probe failed: {e}");
            println!("     Falling back to HTTP-only checks.");
            Transport::Http
        }
    };

    let client = match transport {
        Transport::Http => ReqwestClient::try_new()?,
        Transport::Https => ReqwestClient::try_new_with_local_api_key(
            local_api_key.expect("HTTPS transport requires a local API key"),
        )?,
    };
    let daikin = match transport {
        Transport::Http => Daikin::new(ip_addr, client),
        Transport::Https => Daikin::new_https(ip_addr, client),
    };

    section("Device");
    let info = match daikin.get_info().await {
        Ok(i) => i,
        Err(error) => {
            println!("  ❌ Could not query device info");
            if let Some(e) = error.downcast_ref::<reqwest::Error>() {
                println!("     {e}");
            } else if let Some(e) = error.downcast_ref::<serde_json::Error>() {
                println!("     Invalid response: {e}");
            }
            return Ok(());
        }
    };
    line("Name", &info.name);
    line("MAC", &info.mac);
    line("Firmware", &info.version);

    section("API");
    println!("  ✅ /dsiot/edge.adp_i        (get_info)");
    let status = match daikin.get_status().await {
        Ok(s) => s,
        Err(error) => {
            println!("  ❌ /dsiot/multireq          (get_status)");
            if let Some(e) = error.downcast_ref::<reqwest::Error>() {
                println!("     {e}");
            } else if let Some(e) = error.downcast_ref::<serde_json::Error>() {
                println!("     Invalid response: {e}");
            }
            return Ok(());
        }
    };
    println!("  ✅ /dsiot/multireq          (get_status)");

    section("Current state");
    match &status.power.metadata {
        Metadata::Binary(Binary::Step(_)) => {
            let on = status.power.get_f32().is_some_and(|v| v >= 1.0);
            line("Power", if on { "ON" } else { "OFF" });
        }
        _ => line("Power", format!("⚠️  invalid metadata: {:?}", status.power)),
    }
    match (
        &status.sensors.temperature.metadata,
        status.sensors.temperature.get_f32(),
    ) {
        (Metadata::Binary(Binary::Step(s)), Some(v)) => {
            line(
                "Room temperature",
                format!(
                    "{v} °C  (range {}-{} °C, step {})",
                    s.range().start(),
                    s.range().end(),
                    s.step()
                ),
            );
        }
        _ => line("Room temperature", "⚠️  unavailable"),
    }
    match &status.mode.metadata {
        Metadata::Binary(Binary::Enum(e)) if e.max == "2F00" => {
            line("Mode", format_enum(status.mode.get_enum()));
        }
        _ => line("Mode", format!("⚠️  invalid: {:?}", status.mode)),
    }
    show_setpoint("Cooling setpoint", &status.temperature.cooling);
    show_setpoint("Heating setpoint", &status.temperature.heating);

    section("Power consumption");
    if info.en_ipower {
        match (
            &status.power_consumption.metadata,
            status.power_consumption.get_f32(),
        ) {
            (Metadata::Binary(Binary::Step(_)), Some(v)) => {
                println!("  ✅ Live power measurement");
                line("Current draw", format!("{v} W"));
            }
            _ => {
                println!("  ⚠️  en_ipower=true but no readable value");
            }
        }
    } else {
        println!("  ℹ️  Not available on this device (en_ipower=false)");
    }

    section("Wind (Cooling)");
    let mut warn = false;
    warn |= !show_enum(
        "Speed",
        &status.wind.cooling.speed.metadata,
        "F80C",
        format_enum(status.wind.cooling.speed.get_enum()),
    );
    warn |= !show_enum(
        "Vertical",
        &status.wind.cooling.vertical_direction.metadata,
        "3F808100",
        format_enum(status.wind.cooling.vertical_direction.get_enum()),
    );
    warn |= !show_enum(
        "Horizontal",
        &status.wind.cooling.horizontal_direction.metadata,
        "FD8101",
        format_enum(status.wind.cooling.horizontal_direction.get_enum()),
    );

    println!();
    if warn {
        println!("🙆 Mostly compatible — optional wind features may be limited.");
    } else {
        println!("🎉 Device is perfectly compatible.");
    }

    Ok(())
}

fn format_enum<E: std::fmt::Debug>(e: Option<E>) -> String {
    e.map_or_else(|| "—".to_string(), |v| format!("{v:?}"))
}

fn show_setpoint(label: &str, item: &dsiot::protocol::Item<f32>) {
    match (&item.metadata, item.get_f32()) {
        (Metadata::Binary(Binary::Step(s)), Some(v)) => {
            line(
                label,
                format!(
                    "{v} °C  (range {}-{} °C, step {})",
                    s.range().start(),
                    s.range().end(),
                    s.step()
                ),
            );
        }
        _ => line(label, "⚠️  unavailable"),
    }
}

fn show_enum(label: &str, metadata: &Metadata, expected_max: &str, value: String) -> bool {
    match metadata {
        Metadata::Binary(Binary::Enum(e)) if e.max == expected_max => {
            line(label, value);
            true
        }
        _ => {
            line(label, format!("⚠️  unsupported: {metadata:?}"));
            false
        }
    }
}
