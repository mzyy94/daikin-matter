use super::property::Property;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize, Debug, Clone)]
pub struct DaikinResponse {
    pub responses: Vec<Response>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Response {
    #[serde(rename = "fr")]
    pub from: String,
    #[serde(rename = "pc", deserialize_with = "deserialize_empty_object")]
    pub content: Option<Property>,
    #[serde(rename = "rsc")]
    pub status_code: u32, // response status code
}

fn deserialize_empty_object<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(
        untagged,
        deny_unknown_fields,
        expecting = "object, empty object or null"
    )]
    enum Helper<T> {
        Data(T),
        Empty {},
        Null,
    }
    match Helper::deserialize(deserializer) {
        Ok(Helper::Data(data)) => Ok(Some(data)),
        Ok(_) => Ok(None),
        Err(e) => Err(e),
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
    fn test_empty_object() {
        let json_data = r#"
        {
            "responses": [
                {
                    "fr": "/dsiot/edge/adr_0100.i_power.year_power",
                    "pc": {},
                    "rsc": 4042
                }
            ]
        }
        "#;
        let response: DaikinResponse =
            serde_json::from_str(json_data).expect("Failed to deserialize");
        assert!(response.responses[0].content.is_none());
    }
}
