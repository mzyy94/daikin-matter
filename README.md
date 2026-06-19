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

## Configuration

Each option can be set via CLI flag or environment variable. Env vars are convenient under systemd via `EnvironmentFile=`.

| CLI flag | Environment variable | Description |
|---|---|---|
| (argument) | `DAIKIN_IP_ADDRS` | Space-separated IPv4 addresses. Set empty to auto-discover. |
| `--count <N>` | `DAIKIN_COUNT` | Stop discovery after this many devices. |
| `--timeout <MS>` | `DAIKIN_TIMEOUT` | Discovery timeout in milliseconds. |
| `--data-dir <DIR>` | `DAIKIN_DATA_DIR` | Persistent data directory. |
| `--local-api-key-file <PATH>` | `DAIKIN_LOCAL_API_KEY_FILE` | Path to a file containing the Gen5 raw `localApiKey`. |
| n/a | `DAIKIN_LOCAL_API_KEY` | Raw Gen5 `localApiKey` value (alternative to the file). |

## Installation

For debian/ubuntu:

Download the `.deb` for your architecture from the [Releases Page](https://github.com/mzyy94/daikin-matter/releases) and install:

```bash
$ sudo apt install ./daikin-matter_*.deb
```

The service starts automatically on install. The bridge prints the pairing QR code to the journal on first boot

```bash
$ journalctl -u daikin-matter -b -o cat | grep QRCode -A20
```

For other systems, see [Build](#build).

## Build

```bash
$ cargo install --git https://github.com/mzyy94/daikin-matter --root /usr/local
```

On Linux, the `builtin-mdns` feature is recommended as it does not require Avahi:

```bash
$ cargo install --git https://github.com/mzyy94/daikin-matter --root /usr/local --no-default-features --features builtin-mdns
```


## Debug

```bash
$ RUST_LOG=daikin_matter=debug daikin-matter
```

To check the service status and logs:

```bash
$ sudo systemctl status daikin-matter
$ journalctl -u daikin-matter -f
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
| Power consumption (W) | `ElectricalPowerMeasurement` | ✅ (iOS 27+) | ✅ |
| Wi-Fi signal strength (RSSI) | `WiFiNetworkDiagnostics` | ❌ | ❌ |

Apple Home has limited support for Room Air Conditioner device type. Only basic thermostat and power controls are available. Home Assistant's Matter integration provides access to more features including fan control and sensor readings, but Fan/Dry modes are hidden by the vendor-level UI filtering.

Tested with iOS 27 beta, Home Assistant 2026.4.3, and Daikin AC firmware 3.11.0.

## Compatibility

The app is compatible with year 2022 or later model Daikin Air Conditioners that use the HTTP DSIOT protocol (`adp_kind=4`, `api_ver=2_*`).
It has been tested on 2022-model of [Daikin risora] which has built-in Wi-Fi module.

[Daikin risora]: https://www.ac.daikin.co.jp/kabekake/products/sx_series

> [!NOTE]
> Newer Daikin models ship with a Gen5 Wi-Fi adapter (`adp_kind=5`, `api_ver=3_0`) that exposes only an HTTPS DSIOT endpoint with authentication. These are supported with the `--local-api-key-file` flag. See [Generation5 HTTPS adapters](#generation5-https-adapters) for more details.

To check compatibility, run the command below..

```bash
$ cargo run --example compatibility_check <your device ip address>
# Gen5 HTTPS model
$ cargo run --example compatibility_check -- --local-api-key-file ./local_api_key <your device ip address>
```

![compatibility_check](/docs/compatibility_check.png)

## Generation5 HTTPS adapters

Generation5 adapters (2026 RX / RX-series HTTPS-only adapters) require a local API key.
Only the raw `localApiKey` value is needed; `localKeyID` is not used by this bridge.
The exact extraction path depends on app platform and version, but the key is stored in the official Daikin app data (e.g., extracted from an encrypted iPhone backup).
After exporting or extracting the app data, search for it:

```bash
$ grep -R "localApiKey" path/to/extracted-daikin-app-data
```

Write only the raw key value to a file:

```bash
$ umask 077
$ ${EDITOR:-vi} local_api_key
$ chmod 0600 local_api_key
```

Pass the key file when using explicit IP addresses. The bridge tries HTTPS first and falls back to plain HTTP if HTTPS is unavailable, allowing mixed adapter generations in one command:

```bash
$ daikin-matter --local-api-key-file ./local_api_key 192.168.1.150 192.168.1.151 192.168.1.152 192.168.1.153
```

For systemd, install the key file where only root can read it and point `DAIKIN_LOCAL_API_KEY_FILE` at it in `/etc/daikin-matter/env`:

```bash
$ sudo install -o root -g root -m0600 local_api_key /etc/daikin-matter/local_api_key
$ sudo ${EDITOR:-vi} /etc/daikin-matter/env   # set DAIKIN_LOCAL_API_KEY_FILE=/etc/daikin-matter/local_api_key
$ sudo systemctl restart daikin-matter
```

## License

GPL-3.0
