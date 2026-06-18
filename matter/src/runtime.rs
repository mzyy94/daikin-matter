//! Matter stack assembly and the device polling loop.

use core::pin::pin;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::time::Duration;

use daikin_client::{Daikin, ReqwestClient};
use dsiot::DaikinInfo;

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
use rs_matter::pairing::{DiscoveryCapabilities, qr::QrTextType};
use rs_matter::persist::{DirKvBlobStore, SharedKvBlobStore};
use rs_matter::respond::DefaultResponder;
use rs_matter::sc::pase::MAX_COMM_WINDOW_TIMEOUT_SECS;
use rs_matter::transport::MATTER_SOCKET_BIND_ADDR;
use rs_matter::utils::init::InitMaybeUninit;
use rs_matter::utils::select::Coalesce;
use rs_matter::utils::storage::pooled::PooledBuffers;
use rs_matter::{MATTER_PORT, Matter};

use crate::bridge::{self, BridgeHandler, BridgedMatcher};
use crate::mdns::run_mdns;
use crate::{bridged_info, device, fan_control, humidity, onoff, power, thermostat};

static MATTER: StaticCell<Matter> = StaticCell::new();
static BUFFERS: StaticCell<PooledBuffers<32, IMBuffer>> = StaticCell::new();
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

pub(crate) fn run_matter(
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
    let mut respond = pin!(responder.run::<16, 4>());
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
