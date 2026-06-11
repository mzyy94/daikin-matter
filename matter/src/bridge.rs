use dsiot::protocol::DaikinInfo;
use rs_matter::dm::AttrChangeNotifier;
use rs_matter::dm::clusters::decl::bridged_device_basic_information;
use rs_matter::dm::clusters::decl::electrical_power_measurement;
use rs_matter::dm::clusters::decl::power_topology as rs_power_topology;
use rs_matter::dm::clusters::decl::fan_control as rs_fan_control;
use rs_matter::dm::clusters::decl::relative_humidity_measurement;
use rs_matter::dm::clusters::decl::thermostat as rs_thermostat;
use rs_matter::dm::clusters::decl::wi_fi_network_diagnostics;
use rs_matter::dm::clusters::decl::{identify, on_off};
use rs_matter::dm::clusters::desc::{self, ClusterHandler as _};
use rs_matter::dm::devices::{DEV_TYPE_AGGREGATOR, DEV_TYPE_BRIDGED_NODE};
use rs_matter::dm::subscriptions::Subscriptions;
use rs_matter::dm::{
    Dataver, DeviceType, Endpoint, Handler, InvokeContext, InvokeReply, Matcher, Node,
    NonBlockingHandler, OperationContext, ReadContext, ReadReply, WriteContext,
};
use rs_matter::error::{Error, ErrorCode};
use rs_matter::{clusters, devices, root_endpoint};

use crate::bridged_info::BridgedInfo;
use crate::identify::StubIdentify;
use crate::{device, fan_control, humidity, onoff, power, power_topology, thermostat, wifi_diag};

pub(crate) const DEV_TYPE_ROOM_AC: DeviceType = DeviceType {
    dtype: 0x0072,
    drev: 2,
};

pub(crate) const DEV_TYPE_ELECTRICAL_SENSOR: DeviceType = DeviceType {
    dtype: 0x0510,
    drev: 1,
};

const ROOT_EP: Endpoint<'static> = root_endpoint!(geth);

const AGGREGATOR_EP: Endpoint<'static> = Endpoint {
    id: 1,
    device_types: devices!(DEV_TYPE_AGGREGATOR),
    clusters: clusters!(desc::DescHandler::CLUSTER),
};

const BRIDGED_EP: Endpoint<'static> = Endpoint {
    id: 0, // placeholder, overridden at runtime
    device_types: devices!(DEV_TYPE_ROOM_AC, DEV_TYPE_BRIDGED_NODE),
    clusters: clusters!(
        desc::DescHandler::CLUSTER,
        StubIdentify::CLUSTER,
        BridgedInfo::CLUSTER,
        onoff::OnOffHandler::CLUSTER,
        thermostat::ThermostatHandler::CLUSTER,
        fan_control::FanControlHandler::CLUSTER,
        humidity::HumidityHandler::CLUSTER,
        wifi_diag::WifiDiagHandler::CLUSTER
    ),
};

const BRIDGED_EP_POWER: Endpoint<'static> = Endpoint {
    id: 0, // placeholder, overridden at runtime
    device_types: devices!(
        DEV_TYPE_ROOM_AC,
        DEV_TYPE_ELECTRICAL_SENSOR,
        DEV_TYPE_BRIDGED_NODE
    ),
    clusters: clusters!(
        desc::DescHandler::CLUSTER,
        StubIdentify::CLUSTER,
        BridgedInfo::CLUSTER,
        onoff::OnOffHandler::CLUSTER,
        thermostat::ThermostatHandler::CLUSTER,
        fan_control::FanControlHandler::CLUSTER,
        humidity::HumidityHandler::CLUSTER,
        power_topology::PowerTopologyHandler::CLUSTER,
        power::PowerHandler::CLUSTER,
        wifi_diag::WifiDiagHandler::CLUSTER
    ),
};

pub(crate) fn build_node(devices: &[(u16, bool)]) -> Node<'static> {
    let mut endpoints = vec![ROOT_EP, AGGREGATOR_EP];
    for &(id, has_power) in devices {
        let template = if has_power {
            BRIDGED_EP_POWER
        } else {
            BRIDGED_EP
        };
        endpoints.push(Endpoint { id, ..template });
    }
    Node {
        endpoints: Box::leak(endpoints.into_boxed_slice()),
    }
}

pub(crate) struct BridgedDevice {
    pub(crate) ep_id: u16,
    desc: desc::HandlerAdaptor<desc::DescHandler<'static>>,
    identify: StubIdentify,
    pub(crate) bridged_info: BridgedInfo,
    pub(crate) on_off: onoff::OnOffHandler,
    pub(crate) therm: thermostat::ThermostatHandler,
    pub(crate) fan_ctl: fan_control::FanControlHandler,
    pub(crate) humidity: humidity::HumidityHandler,
    pub(crate) power: Option<power::PowerHandler>,
    pub(crate) power_topology: Option<power_topology::PowerTopologyHandler>,
    pub(crate) wifi_diag: wifi_diag::WifiDiagHandler,
    pub(crate) device: device::Device,
}

impl BridgedDevice {
    pub(crate) fn new(
        ep_id: u16,
        rand: &mut impl rand::RngCore,
        bridged_info: BridgedInfo,
        device: device::Device,
        info: DaikinInfo,
    ) -> Self {
        let (power, power_topology) = if info.en_ipower {
            (
                Some(power::PowerHandler::new(
                    Dataver::new_rand(rand),
                    device.clone(),
                )),
                Some(power_topology::PowerTopologyHandler::new(Dataver::new_rand(
                    rand,
                ))),
            )
        } else {
            (None, None)
        };
        let wifi_diag =
            wifi_diag::WifiDiagHandler::new(Dataver::new_rand(rand), info, device.clone());
        Self {
            ep_id,
            desc: desc::DescHandler::new(Dataver::new_rand(rand)).adapt(),
            identify: StubIdentify::new(Dataver::new_rand(rand)),
            bridged_info,
            on_off: onoff::OnOffHandler::new(Dataver::new_rand(rand), device.clone()),
            therm: thermostat::ThermostatHandler::new(Dataver::new_rand(rand), device.clone()),
            fan_ctl: fan_control::FanControlHandler::new(Dataver::new_rand(rand), device.clone()),
            humidity: humidity::HumidityHandler::new(Dataver::new_rand(rand), device.clone()),
            power,
            power_topology,
            wifi_diag,
            device,
        }
    }
}

pub(crate) struct BridgeHandler {
    pub(crate) devices: Vec<BridgedDevice>,
    pub(crate) subscriptions: &'static Subscriptions,
}

impl BridgeHandler {
    fn find(&self, ep_id: u16) -> Option<&BridgedDevice> {
        self.devices.iter().find(|d| d.ep_id == ep_id)
    }

    fn notify_all_clusters(&self, ep: u16) {
        self.subscriptions
            .notify_attr_changed(ep, onoff::OnOffHandler::CLUSTER.id, 0);
        self.subscriptions
            .notify_attr_changed(ep, thermostat::ThermostatHandler::CLUSTER.id, 0);
        self.subscriptions
            .notify_attr_changed(ep, fan_control::FanControlHandler::CLUSTER.id, 0);
        self.subscriptions
            .notify_attr_changed(ep, humidity::HumidityHandler::CLUSTER.id, 0);
        if self.find(ep).is_some_and(|d| d.power.is_some()) {
            self.subscriptions
                .notify_attr_changed(ep, power::PowerHandler::CLUSTER.id, 0);
        }
    }
}

/// Matches any bridged endpoint (id >= 2).
pub(crate) struct BridgedMatcher;

impl Matcher for BridgedMatcher {
    fn matches(&self, ctx: impl OperationContext) -> bool {
        ctx.endpt() >= 2
    }
}

impl Handler for BridgeHandler {
    fn read(&self, ctx: impl ReadContext, reply: impl ReadReply) -> Result<(), Error> {
        let ep = ctx.endpt();
        let cl = ctx.cluster();
        let dev = self
            .find(ep)
            .ok_or(Error::from(ErrorCode::EndpointNotFound))?;

        if cl == desc::DescHandler::CLUSTER.id {
            dev.desc.read(ctx, reply)
        } else if cl == StubIdentify::CLUSTER.id {
            identify::HandlerAdaptor(&dev.identify).read(ctx, reply)
        } else if cl == BridgedInfo::CLUSTER.id {
            bridged_device_basic_information::HandlerAdaptor(&dev.bridged_info).read(ctx, reply)
        } else if cl == onoff::OnOffHandler::CLUSTER.id {
            on_off::HandlerAdaptor(&dev.on_off).read(ctx, reply)
        } else if cl == thermostat::ThermostatHandler::CLUSTER.id {
            rs_thermostat::HandlerAdaptor(&dev.therm).read(ctx, reply)
        } else if cl == fan_control::FanControlHandler::CLUSTER.id {
            rs_fan_control::HandlerAdaptor(&dev.fan_ctl).read(ctx, reply)
        } else if cl == humidity::HumidityHandler::CLUSTER.id {
            relative_humidity_measurement::HandlerAdaptor(&dev.humidity).read(ctx, reply)
        } else if cl == power::PowerHandler::CLUSTER.id {
            match &dev.power {
                Some(p) => electrical_power_measurement::HandlerAdaptor(p).read(ctx, reply),
                None => Err(ErrorCode::ClusterNotFound.into()),
            }
        } else if cl == power_topology::PowerTopologyHandler::CLUSTER.id {
            match &dev.power_topology {
                Some(p) => rs_power_topology::HandlerAdaptor(p).read(ctx, reply),
                None => Err(ErrorCode::ClusterNotFound.into()),
            }
        } else if cl == wifi_diag::WifiDiagHandler::CLUSTER.id {
            wi_fi_network_diagnostics::HandlerAdaptor(&dev.wifi_diag).read(ctx, reply)
        } else {
            Err(ErrorCode::ClusterNotFound.into())
        }
    }

    fn write(&self, ctx: impl WriteContext) -> Result<(), Error> {
        let ep = ctx.endpt();
        let cl = ctx.cluster();
        let dev = self
            .find(ep)
            .ok_or(Error::from(ErrorCode::EndpointNotFound))?;

        let result = if cl == BridgedInfo::CLUSTER.id {
            bridged_device_basic_information::HandlerAdaptor(&dev.bridged_info).write(ctx)
        } else if cl == StubIdentify::CLUSTER.id {
            identify::HandlerAdaptor(&dev.identify).write(ctx)
        } else if cl == onoff::OnOffHandler::CLUSTER.id {
            on_off::HandlerAdaptor(&dev.on_off).write(ctx)
        } else if cl == thermostat::ThermostatHandler::CLUSTER.id {
            rs_thermostat::HandlerAdaptor(&dev.therm).write(ctx)
        } else if cl == fan_control::FanControlHandler::CLUSTER.id {
            rs_fan_control::HandlerAdaptor(&dev.fan_ctl).write(ctx)
        } else {
            Err(ErrorCode::AttributeNotFound.into())
        };
        if result.is_ok() {
            self.notify_all_clusters(ep);
        }
        result
    }

    fn invoke(&self, ctx: impl InvokeContext, reply: impl InvokeReply) -> Result<(), Error> {
        let ep = ctx.endpt();
        let cl = ctx.cluster();
        let dev = self
            .find(ep)
            .ok_or(Error::from(ErrorCode::EndpointNotFound))?;

        let result = if cl == StubIdentify::CLUSTER.id {
            identify::HandlerAdaptor(&dev.identify).invoke(ctx, reply)
        } else if cl == onoff::OnOffHandler::CLUSTER.id {
            on_off::HandlerAdaptor(&dev.on_off).invoke(ctx, reply)
        } else if cl == thermostat::ThermostatHandler::CLUSTER.id {
            rs_thermostat::HandlerAdaptor(&dev.therm).invoke(ctx, reply)
        } else if cl == fan_control::FanControlHandler::CLUSTER.id {
            rs_fan_control::HandlerAdaptor(&dev.fan_ctl).invoke(ctx, reply)
        } else if cl == wifi_diag::WifiDiagHandler::CLUSTER.id {
            wi_fi_network_diagnostics::HandlerAdaptor(&dev.wifi_diag).invoke(ctx, reply)
        } else {
            Err(ErrorCode::CommandNotFound.into())
        };
        if result.is_ok() {
            self.notify_all_clusters(ep);
        }
        result
    }
}

impl NonBlockingHandler for BridgeHandler {}
