//! mDNS responder variants gated by Cargo feature.

use rs_matter::Matter;
use rs_matter::error::Error;

#[cfg(all(
    feature = "astro-dnssd",
    not(feature = "avahi"),
    not(feature = "builtin-mdns")
))]
pub(crate) async fn run_mdns(matter: &Matter<'_>) -> Result<(), Error> {
    rs_matter::transport::network::mdns::astro::AstroMdns::new()
        .run(matter)
        .await
}

#[cfg(all(feature = "avahi", not(feature = "builtin-mdns")))]
pub(crate) async fn run_mdns(matter: &Matter<'_>) -> Result<(), Error> {
    let connection = rs_matter::utils::zbus::Connection::system().await.unwrap();
    rs_matter::transport::network::mdns::avahi::AvahiMdns::new(connection)
        .run(matter)
        .await
}

#[cfg(feature = "builtin-mdns")]
pub(crate) async fn run_mdns(matter: &Matter<'_>) -> Result<(), Error> {
    use nix::net::if_::InterfaceFlags;
    use nix::sys::socket::SockaddrIn6;
    use rs_matter::crypto::default_crypto;
    use rs_matter::dm::clusters::dev_att::DeviceAttestation;
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
