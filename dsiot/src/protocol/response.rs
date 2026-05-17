use super::property::Property;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Deserializer, de};

#[derive(Deserialize, Debug, Clone)]
pub struct DaikinResponse {
    pub responses: Vec<Response>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Response {
    #[serde(rename = "fr")]
    pub from: String,
    #[serde(rename = "pc")]
    pub content: Option<Property>,
    #[serde(rename = "rsc", deserialize_with = "verify_status_code")]
    pub status_code: u32, // response status code
}

fn verify_status_code<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let value: u32 = Deserialize::deserialize(deserializer)?;
    if value / 10 == 200 {
        Ok(value)
    } else {
        Err(de::Error::custom(format!("Invalid status code: {value}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_status_code() {
        let json_data = r#"
        {
            "responses": [
                {
                    "fr": "/dsiot/edge/adr_0100.dgc_status",
                    "pt": 1,
                    "pc": { "pn": "1234", "pt": 3, "pv": "ok", "md": { "pt": "s" } },
                    "rsc": 2000
                }
            ]
        }
        "#;
        let response: DaikinResponse =
            serde_json::from_str(json_data).expect("Failed to deserialize");
        assert_eq!(response.responses[0].status_code, 2000);
    }

    #[test]
    fn test_invalid_status_code() {
        let json_data = r#"
        {
            "responses": [
                {
                    "fr": "/dsiot/edge/adr_0100.dgc_status",
                    "pt": 1,
                    "pc": {"pn": "1234","pt": 3,"pv": "ok","md": {"pt": "s"}},
                    "rsc": 4041
                }
            ]
        }
        "#;
        let result: Result<DaikinResponse, serde_json::Error> = serde_json::from_str(json_data);
        assert!(result.is_err());
    }
}
