use serde_derive::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Deserialize, Serialize, Clone)]
pub struct HOSIncomingReq {
    #[serde(rename = "type")]
    pub _type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,

    // type = "response"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

#[derive(Debug, PartialEq, Deserialize, Serialize, Clone)]
pub struct HOSOutgoingReq {
    #[serde(rename = "type")]
    pub _type: String,
    pub method: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

pub fn hos_request(method: &str, url: &str, id: String) -> Result<String, serde_json::Error> {
    serde_json::to_string(&HOSOutgoingReq {
        _type: "request".to_string(),
        method: method.to_string(),
        url: url.to_string(),
        id: Some(id),
    })
}

#[derive(Debug, PartialEq, Deserialize, Serialize, Clone)]
pub struct HOSConnectionList {
    pub connections: Vec<(String, String)>,
}
