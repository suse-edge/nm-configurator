use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Host {
    pub(crate) hostname: String,
    pub(crate) interfaces: Vec<Interface>,
}

#[derive(Serialize, Deserialize, Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub struct Interface {
    pub(crate) logical_name: String,
    pub(crate) mac_address: String,
}
