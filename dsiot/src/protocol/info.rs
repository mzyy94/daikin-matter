use super::property::Item;
use super::response::DaikinResponse;
use alloc::string::String;
use serde::{Deserialize, Deserializer, de};

#[derive(Deserialize, Debug, Clone)]
pub struct DaikinInfo {
    pub name: String,
    pub mac: String,
    #[serde(rename = "ver", deserialize_with = "parse_version")]
    pub version: String,
    #[serde(deserialize_with = "parse_edid")]
    pub edid: u64,
    #[serde(default)]
    pub en_ipower: bool,
    #[serde(skip)]
    pub rssi: Option<i8>,
    #[serde(skip)]
    pub ssid: Option<String>,
    #[serde(skip)]
    pub security_type: Option<String>,
}

fn parse_version<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let s = String::deserialize(deserializer)?;
    Ok(s.replace('_', "."))
}

fn parse_edid<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let s = String::deserialize(deserializer)?;
    str2edid(&s).ok_or_else(|| de::Error::custom("Invalid EDID format"))
}

fn str2edid(edid: &str) -> Option<u64> {
    let mut bytes = [0u8; 8];
    match hex::decode_to_slice(edid, &mut bytes) {
        Ok(_) => {}
        Err(_) => return None,
    };
    Some(u64::from_be_bytes(bytes))
}

impl From<DaikinResponse> for DaikinInfo {
    fn from(res: DaikinResponse) -> Self {
        DaikinInfo {
            name: get_prop!(res."/dsiot/edge.adp_d".name .to_string()).unwrap_or_default(),
            mac: get_prop!(res."/dsiot/edge.adp_i".mac .to_string()).unwrap_or_default(),
            version: get_prop!(res."/dsiot/edge.adp_i".ver .to_string())
                .unwrap_or_default()
                .replace('_', "."),
            edid: str2edid(
                &get_prop!(res."/dsiot/edge.adp_i".edid .to_string()).unwrap_or_default(),
            )
            .unwrap_or(0),
            en_ipower: {
                let v: Item<f32> = get_prop!(res."/dsiot/edge.adp_i".func.en_ipower);
                v.get_int() == Some(1)
            },
            rssi: {
                let v: Item<f32> = get_prop!(res."/dsiot/edge.adp_r".wlan_info.rssi);
                v.get_int().map(|v| v as i8)
            },
            ssid: {
                let s = get_prop!(res."/dsiot/edge.adp_r".wlan_info.ssid .to_string());
                s.filter(|s| !s.is_empty())
            },
            security_type: {
                let s = get_prop!(res."/dsiot/edge.adp_r".wlan_info.sec_type .to_string());
                s.filter(|s| !s.is_empty())
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getter() {
        let res: DaikinResponse = serde_json::from_str(include_str!("../fixtures/info.json"))
            .expect("Invalid JSON file.");
        let info: DaikinInfo = res.into();

        assert_eq!(info.name, "display_name");
        assert_eq!(info.mac, "00005E005342");
        assert_eq!(info.version, "2.7.0");
        assert_eq!(info.edid, 19088743);
        assert!(info.en_ipower);
        assert_eq!(info.rssi, Some(-30));
        assert_eq!(info.ssid.as_deref(), Some("WLAN_SSID"));
        assert_eq!(info.security_type.as_deref(), Some("WPA2"));
    }

    #[test]
    fn serde() {
        let text = "ret=OK,type=GPF,cdev=RA,protocol=DGC,reg=jp,ver=2_7_0,rev=aabbcc00,comm_err=0,lpw_flag=0,adp_kind=4,mac=00005E005342,ssid=DaikinAP12345,adp_mode=ap_run,method=polling,name=%64%69%73%70%6c%61%79%5f%6e%61%6d%65,icon=23,edid=0000000001234567,sw_id=1900294D,api_ver=2_2";
        let info: DaikinInfo = serde_qs::from_str(&text.replace(',', "&")).unwrap();

        assert_eq!(info.name, "display_name");
        assert_eq!(info.mac, "00005E005342");
        assert_eq!(info.version, "2.7.0");
        assert_eq!(info.edid, 19088743);
    }
}
