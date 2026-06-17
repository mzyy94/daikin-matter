Daikin Matter Bridge
---

<a href="https://github.com/mzyy94/daikin-matter/releases"><img src="https://img.shields.io/github/release/mzyy94/daikin-matter.svg" alt="Latest Release"></a>
<a href="https://github.com/mzyy94/daikin-matter/actions"><img src="https://github.com/mzyy94/daikin-matter/actions/workflows/build.yml/badge.svg" alt="Build Status"></a>

Control Daikin Air Conditioner via Matter. Compatible with new Daikin API. ([Legacy API] is not supported.)

> [!NOTE]
> v0.3 and later use Matter. For HomeKit support, see [v0.2.4](https://github.com/mzyy94/daikin-homekit/releases/tag/v0.2.4) and [homekit branch](https://github.com/mzyy94/daikin-homekit/tree/homekit).

![daikin-matter](/docs/daikin-matter.png)


[Legacy API]: https://github.com/ael-code/daikin-control/wiki/API-System


## Usage

```
$ daikin-matter
```

Open your Matter controller (Apple Home, Google Home, Home Assistant, etc.) and commission the bridge using the QR code displayed in the terminal.

By default, a device is automatically discovered at startup when run the command without any arguments.
If you want to specify a device, run with the IP address as an argument. Run `daikin-matter -h` for more detail.

## Installation

Get and unarchive latest release from [Releases Page](https://github.com/mzyy94/daikin-matter/releases) and install it with the following command.

```bash
$ install -m0755 daikin-matter /usr/local/bin/
```

## Build

```bash
$ cargo install --git https://github.com/mzyy94/daikin-matter --root /usr/local
```

On Linux, the `builtin-mdns` feature is recommended as it does not require Avahi:

```bash
$ cargo install --git https://github.com/mzyy94/daikin-matter --root /usr/local --no-default-features --features builtin-mdns
```

## Running as a daemon (systemd)

A systemd service file is included in the repository. To run `daikin-matter` as a background service on Linux:

```bash
$ sudo curl https://github.com/mzyy94/daikin-matter/raw/refs/heads/master/daikin-matter.service -LO --output-dir /etc/systemd/system/
$ sudo systemctl daemon-reload
$ sudo systemctl enable --now daikin-matter
```

To check the service status and logs:

```bash
$ sudo systemctl status daikin-matter
$ journalctl -u daikin-matter -f
```

## Debug

```bash
$ RUST_LOG=daikin_matter=debug daikin-matter
```

## Controller support

The bridge exposes the following Matter clusters for each air conditioner:

| Feature | Cluster | Apple Home | Home Assistant |
|---|---|---|---|
| Power on/off | `OnOff` | ✅ | ✅ |
| Mode: Cool / Heat / Auto | `Thermostat` | ✅ | ✅ |
| Mode: Fan / Dry | `Thermostat` | ❌ | ❌ |
| Target temperature (not available in Auto mode) | `Thermostat` | ✅ | ✅ |
| Room temperature | `Thermostat` | ✅ | ✅ |
| Outdoor temperature | `Thermostat` | ❌ | ✅ |
| Fan speed | `FanControl` | ❌ | ✅ |
| Swing (vertical/horizontal, toggles with auto) | `FanControl` | ❌ | ✅ |
| Wind direction | (not in cluster) | ❌ | ❌ |
| Humidity | `RelativeHumidityMeasurement` | ❌ | ✅ |
| Power consumption (W) | `ElectricalPowerMeasurement` | ❌ | ✅ |
| Wi-Fi signal strength (RSSI) | `WiFiNetworkDiagnostics` | ❌ | ❌ |

Apple Home has limited support for Room Air Conditioner device type. Only basic thermostat and power controls are available. Home Assistant's Matter integration provides access to more features including fan control and sensor readings, but Fan/Dry modes are hidden by the vendor-level UI filtering.

Tested with iOS 26.4.2, Home Assistant 2026.4.3, and Daikin AC firmware 3.11.0.

## Compatibility

The app is compatible with year 2022 or later model Daikin Air Conditioners that use the HTTP DSIOT protocol (`adp_kind=4`, `api_ver=2_*`).
It has been tested on [Daikin risora] which has built-in Wi-Fi modules and an IR remote control like the following.

[Daikin risora]: https://www.ac.daikin.co.jp/kabekake/products/sx_series

<img alt="risora ir remote display" src="/docs/remote.png" width="540">

> [!WARNING]
> Newer Daikin models ship with a different Wi-Fi adapter (`adp_kind=5`, `api_ver=3_0`) that exposes only an HTTPS DSIOT endpoint with authentication. These devices are **not yet supported**.

To check compatibility, run the command below.

```bash
$ cargo run --example compatibility_check <your device ip address>
```

![compatibility_check](/docs/compatibility_check.png)

## License

GPL-3.0
