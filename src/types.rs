use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Host {
    pub(crate) hostname: String,
    pub(crate) interfaces: Vec<Interface>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Interface {
    pub(crate) logical_name: String,
    #[serde(default)]
    pub(crate) connection_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub(crate) mac_address: Option<String>,
    pub(crate) interface_type: String,
}
