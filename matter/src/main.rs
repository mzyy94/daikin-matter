#[macro_use]
extern crate log;

mod bridge;
mod bridged_info;
mod device;
mod fan_control;
mod humidity;
mod identify;
mod onoff;
mod power;
mod power_topology;
mod thermostat;
mod wifi_diag;

use core::pin::pin;
use std::collections::HashMap;
use std::net::{Ipv4Addr, UdpSocket};
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use daikin_client::{Daikin, ReqwestClient, discovery};
use dsiot::DaikinInfo;
use futures_lite::StreamExt;

use embassy_futures::select::{select, select4};
use static_cell::StaticCell;

use rs_matter::crypto::{Crypto, CryptoSensitive, CryptoSensitiveRef, default_crypto};
use rs_matter::dm::clusters::basic_info::BasicInfoConfig;
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::clusters::dev_att::DeviceAttestation;
use rs_matter::dm::clusters::net_comm::SharedNetworks;
use rs_matter::dm::devices::test::TEST_DEV_ATT;
use rs_matter::dm::events::NoEvents;
use rs_matter::dm::networks::eth::EthNetwork;
use rs_matter::dm::networks::unix::UnixNetifs;
use rs_matter::dm::subscriptions::Subscriptions;
use rs_matter::dm::{
    Async, AttrChangeNotifier, DataModel, DataModelHandler, Dataver, EpClMatcher, IMBuffer, Node,
    endpoints,
};
use rs_matter::error::Error;
use rs_matter::pairing::{DiscoveryCapabilities, qr::QrTextType};
use rs_matter::persist::{DirKvBlobStore, SharedKvBlobStore};
use rs_matter::respond::DefaultResponder;
use rs_matter::sc::pase::MAX_COMM_WINDOW_TIMEOUT_SECS;
use rs_matter::transport::MATTER_SOCKET_BIND_ADDR;
use rs_matter::utils::init::InitMaybeUninit;
use rs_matter::utils::select::Coalesce;
use rs_matter::utils::storage::pooled::PooledBuffers;
use rs_matter::{MATTER_PORT, Matter};

use bridge::{BridgeHandler, BridgedMatcher};

static MATTER: StaticCell<Matter> = StaticCell::new();
static BUFFERS: StaticCell<PooledBuffers<10, IMBuffer>> = StaticCell::new();
static SUBSCRIPTIONS: StaticCell<Subscriptions> = StaticCell::new();
static KV_BUF: StaticCell<[u8; 4096]> = StaticCell::new();

const COMM_DATA: rs_matter::BasicCommData = rs_matter::BasicCommData {
    password: CryptoSensitive::new_from_ref(CryptoSensitiveRef::new(&20230420_u32.to_le_bytes())),
    discriminator: 94,
};

const BRIDGE_DEV_DET: BasicInfoConfig<'static> = BasicInfoConfig {
    vid: 0xfff1,
    pid: 0x8001,
    product_name: "Daikin Matter Bridge",
    vendor_name: "daikin-matter",
    device_name: "Daikin Matter Bridge",
    hw_ver: 1,
    hw_ver_str: "1",
    sw_ver: 1,
    sw_ver_str: env!("CARGO_PKG_VERSION"),
    serial_no: "daikin-matter",
    product_label: "Daikin Matter Bridge",
    product_url: env!("CARGO_PKG_REPOSITORY"),
    ..BasicInfoConfig::new()
};

fn dm_handler<'a>(
    mut rand: impl rand::RngCore,
    bridge: &'a BridgeHandler,
    node: Node<'static>,
) -> impl DataModelHandler + 'a {
    let agg_desc_dataver = Dataver::new_rand(&mut rand);

    (
        node,
        endpoints::EthSysHandlerBuilder::new()
            .netif_diag(&UnixNetifs)
            .build(rand)
            .chain(
                EpClMatcher::new(Some(1), Some(desc::DescHandler::CLUSTER.id)),
                Async(desc::DescHandler::new_aggregator(agg_desc_dataver).adapt()),
            )
            .chain(BridgedMatcher, Async(bridge)),
    )
}

#[cfg(all(
    feature = "astro-dnssd",
    not(feature = "avahi"),
    not(feature = "builtin-mdns")
))]
async fn run_mdns(matter: &Matter<'_>) -> Result<(), Error> {
    rs_matter::transport::network::mdns::astro::AstroMdns::new()
        .run(matter)
        .await
}

#[cfg(all(feature = "avahi", not(feature = "builtin-mdns")))]
async fn run_mdns(matter: &Matter<'_>) -> Result<(), Error> {
    let connection = rs_matter::utils::zbus::Connection::system().await.unwrap();
    rs_matter::transport::network::mdns::avahi::AvahiMdns::new(connection)
        .run(matter)
        .await
}

#[cfg(feature = "builtin-mdns")]
async fn run_mdns(matter: &Matter<'_>) -> Result<(), Error> {
    use nix::net::if_::InterfaceFlags;
    use nix::sys::socket::SockaddrIn6;
    use rs_matter::dm::devices::test::TEST_DEV_ATT;
    use rs_matter::error::ErrorCode;
    use rs_matter::transport::network::mdns::builtin::{BuiltinMdns, Host};
    use rs_matter::transport::network::mdns::{
        MDNS_IPV4_BROADCAST_ADDR, MDNS_IPV6_BROADCAST_ADDR, MDNS_SOCKET_DEFAULT_BIND_ADDR,
    };
    use rs_matter::transport::network::{Ipv4Addr as MIpv4Addr, Ipv6Addr as MIpv6Addr};
    use socket2::{Domain, Protocol, Socket, Type};
    use std::net::UdpSocket as StdUdpSocket;

    let crypto = default_crypto(rand::thread_rng(), TEST_DEV_ATT.dac_priv_key());

    let interfaces = || {
        nix::ifaddrs::getifaddrs().unwrap().filter(|ia| {
            ia.flags
                .contains(InterfaceFlags::IFF_UP | InterfaceFlags::IFF_BROADCAST)
                && !ia
                    .flags
                    .intersects(InterfaceFlags::IFF_LOOPBACK | InterfaceFlags::IFF_POINTOPOINT)
        })
    };

    let (iname, ip, ipv6) = interfaces()
        .filter_map(|ia| {
            ia.address
                .and_then(|addr| addr.as_sockaddr_in6().map(SockaddrIn6::ip))
                .map(|ipv6| (ia.interface_name, ipv6))
        })
        .filter_map(|(iname, ipv6)| {
            interfaces()
                .filter(|ia2| ia2.interface_name == iname)
                .find_map(|ia2| {
                    ia2.address
                        .and_then(|addr| addr.as_sockaddr_in().map(|addr| addr.ip()))
                        .map(|ip: std::net::Ipv4Addr| (iname.clone(), ip, ipv6))
                })
        })
        .next()
        .ok_or_else(|| {
            error!("Cannot find network interface suitable for mDNS broadcasting");
            Error::new(ErrorCode::StdIoError)
        })?;

    info!("Will use network interface {iname} with {ip}/{ipv6} for mDNS");

    let ipv4_addr: MIpv4Addr = ip.octets().into();
    let ipv6_addr: MIpv6Addr = ipv6.octets().into();

    let socket = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.set_only_v6(false)?;
    socket.bind(&MDNS_SOCKET_DEFAULT_BIND_ADDR.into())?;
    let socket = async_io::Async::<StdUdpSocket>::new_nonblocking(socket.into())?;

    socket
        .get_ref()
        .join_multicast_v6(&MDNS_IPV6_BROADCAST_ADDR, 0)?;
    socket
        .get_ref()
        .join_multicast_v4(&MDNS_IPV4_BROADCAST_ADDR, &ipv4_addr)?;

    BuiltinMdns::new()
        .run(
            &socket,
            &socket,
            &Host {
                hostname: "daikin-matter",
                ip: ipv4_addr,
                ipv6: ipv6_addr,
            },
            Some(ipv4_addr),
            Some(0),
            matter,
            crypto,
        )
        .await
}

#[derive(Parser)]
#[command(author, version, about)]
struct Cli {
    /// IPv4 address of Daikin AC
    #[arg(value_name = "ip_address")]
    ip_addrs: Vec<Ipv4Addr>,

    /// Discovery timeout in milliseconds
    #[arg(long, default_value = "3000")]
    timeout: u64,

    /// Expected number of devices to discover
    #[arg(long, default_value = "128", hide_default_value = true)]
    count: usize,

    /// Directory to store persistent data (pairing, fabrics, etc.)
    #[arg(long, value_name = "DIR")]
    data_dir: Option<PathBuf>,
}

fn default_data_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("daikin-matter")
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("daikin_matter=debug,rs_matter=info"),
    )
    .init();

    let cli = Cli::parse();

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
                let dk = Daikin::new(*ip, ReqwestClient::try_new()?);
                let info = dk.get_info().await?;
                info!(
                    "Device: {} (MAC: {}, EDID: {})",
                    info.name, info.mac, info.edid
                );
                let status = dk.get_status().await?;
                debug!("Status: {:?}", status);
                conns.push((dk, info));
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
        .spawn(move || run_matter(connections, rt_handle, data_dir))
        .unwrap();

    thread.join().unwrap()
}

fn run_matter(
    connections: Vec<(Daikin<ReqwestClient>, DaikinInfo)>,
    rt_handle: tokio::runtime::Handle,
    data_dir: PathBuf,
) -> anyhow::Result<()> {
    let matter = MATTER.uninit().init_with(Matter::init(
        &BRIDGE_DEV_DET,
        COMM_DATA,
        &TEST_DEV_ATT,
        MATTER_PORT,
    ));

    let kv_buf = KV_BUF.uninit().init_zeroed().as_mut_slice();
    let mut kv = DirKvBlobStore::new(data_dir);
    futures_lite::future::block_on(matter.load_persist(&mut kv, kv_buf))?;

    let buffers = BUFFERS.uninit().init_with(PooledBuffers::init(0));
    let subscriptions: &Subscriptions = SUBSCRIPTIONS.uninit().init_with(Subscriptions::init());

    let crypto = default_crypto(rand::thread_rng(), TEST_DEV_ATT.dac_priv_key());
    let mut rand = crypto.rand()?;

    let mut devices = Vec::with_capacity(connections.len());
    for (dk, info) in connections {
        let ep_id = (info.edid & 0xFFFF) as u16;
        assert!(
            ep_id >= 2,
            "edid-derived endpoint ID {ep_id} conflicts with root/aggregator"
        );
        let device = device::Device::new(dk, rt_handle.clone());
        let bridged_info =
            bridged_info::BridgedInfo::new(Dataver::new_rand(&mut rand), &info, device.clone());
        info!(
            "Bridged endpoint {ep_id}: {} (power: {})",
            info.name, info.en_ipower
        );
        devices.push(bridge::BridgedDevice::new(
            ep_id,
            &mut rand,
            bridged_info,
            device,
            info,
        ));
    }
    let ep_devs: Vec<(u16, bool)> = devices
        .iter()
        .map(|d| (d.ep_id, d.power.is_some()))
        .collect();
    let bridge_handler = BridgeHandler { devices };
    let node = bridge::build_node(&ep_devs);

    let events = NoEvents::new();

    let dm = DataModel::new(
        matter,
        &crypto,
        buffers,
        subscriptions,
        &events,
        dm_handler(rand, &bridge_handler, node),
        SharedKvBlobStore::new(kv, kv_buf),
        SharedNetworks::new(EthNetwork::new_default()),
    );

    let responder = DefaultResponder::new(&dm);
    let mut respond = pin!(responder.run::<8, 4>());
    let mut dm_job = pin!(dm.run());

    let socket = async_io::Async::<UdpSocket>::bind(MATTER_SOCKET_BIND_ADDR)?;

    let mut mdns = pin!(run_mdns(matter));
    let mut transport = pin!(matter.run(&crypto, &socket, &socket, &socket));

    if !matter.is_commissioned() {
        matter.print_standard_qr_text(DiscoveryCapabilities::IP)?;
        matter.print_standard_qr_code(QrTextType::Unicode, DiscoveryCapabilities::IP)?;
        dm.open_basic_comm_window(MAX_COMM_WINDOW_TIMEOUT_SECS)?;
    }

    info!("Matter stack running ({} device(s))", ep_devs.len());
    let mut poll = pin!(async {
        let mut was_reachable: HashMap<u16, bool> = HashMap::new();
        let mut prev: HashMap<u16, dsiot::DaikinStatus> = HashMap::new();
        loop {
            async_io::Timer::after(Duration::from_secs(30)).await;
            for dev in &bridge_handler.devices {
                let reachable_before = was_reachable.get(&dev.ep_id).copied().unwrap_or(true);
                match dev.device.get_status() {
                    Ok(status) => {
                        let old = prev.get(&dev.ep_id);
                        let mut changed = Vec::new();
                        if old.is_none_or(|o| o.power != status.power || o.mode != status.mode) {
                            dev.on_off.dataver.changed();
                            dm.notify_attr_changed(dev.ep_id, onoff::OnOffHandler::CLUSTER.id, 0);
                            changed.push("OnOff");
                        }
                        if old.is_none_or(|o| {
                            o.mode != status.mode
                                || o.temperature != status.temperature
                                || o.sensors.temperature != status.sensors.temperature
                                || o.sensors.outdoor_temperature
                                    != status.sensors.outdoor_temperature
                        }) {
                            dev.therm.dataver.changed();
                            dm.notify_attr_changed(
                                dev.ep_id,
                                thermostat::ThermostatHandler::CLUSTER.id,
                                0,
                            );
                            changed.push("Thermostat");
                        }
                        if old.is_none_or(|o| o.wind != status.wind || o.mode != status.mode) {
                            dev.fan_ctl.dataver.changed();
                            dm.notify_attr_changed(
                                dev.ep_id,
                                fan_control::FanControlHandler::CLUSTER.id,
                                0,
                            );
                            changed.push("FanControl");
                        }
                        if old.is_none_or(|o| o.sensors.humidity != status.sensors.humidity) {
                            dev.humidity.dataver.changed();
                            dm.notify_attr_changed(
                                dev.ep_id,
                                humidity::HumidityHandler::CLUSTER.id,
                                0,
                            );
                            changed.push("Humidity");
                        }
                        if let Some(ref p) = dev.power
                            && old.is_none_or(|o| o.power_consumption != status.power_consumption)
                        {
                            p.dataver.changed();
                            dm.notify_attr_changed(dev.ep_id, power::PowerHandler::CLUSTER.id, 0);
                            changed.push("Power");
                        }
                        if changed.is_empty() {
                            debug!("Poll ep {}: no changes", dev.ep_id);
                        } else {
                            debug!("Poll ep {}: notified [{}]", dev.ep_id, changed.join(", "));
                        }
                        prev.insert(dev.ep_id, status);
                    }
                    Err(e) => warn!("Poll failed (ep {}): {e}", dev.ep_id),
                }
                let reachable_now = dev.device.is_reachable();
                if reachable_now != reachable_before {
                    dev.bridged_info.dataver.changed();
                    dm.notify_attr_changed(dev.ep_id, bridged_info::BridgedInfo::CLUSTER.id, 0);
                    info!(
                        "Poll ep {}: reachable {} → {}",
                        dev.ep_id, reachable_before, reachable_now
                    );
                }
                was_reachable.insert(dev.ep_id, reachable_now);
            }
        }
    });

    let mut core = pin!(select4(&mut transport, &mut mdns, &mut respond, &mut dm_job).coalesce());
    let all = select(&mut core, &mut poll).coalesce();
    futures_lite::future::block_on(all)?;

    Ok(())
}
