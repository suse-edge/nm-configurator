use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};
use log::{info, warn};
use nmstate::{InterfaceType, NetworkState};
use serde::Serialize;

const HOST_MAPPING_FILE: &str = "host_config.yaml";

#[derive(Serialize)]
pub struct HostInterfaces {
    hostname: String,
    interfaces: Vec<Interface>,
}

#[derive(Serialize)]
pub struct Interface {
    logical_name: String,
    mac_address: String,
}

pub(crate) fn generate(config_dir: &str) -> Result<(), anyhow::Error> {
    for entry in fs::read_dir(config_dir)? {
        let entry = entry?;
        let path = entry.path();

        if entry.metadata()?.is_dir() {
            warn!("Ignoring unexpected dir: {path:?}");
            continue;
        }

        info!("Generating config from {path:?}...");

        let hostname = path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("Invalid file path"))?;

        let data = fs::read_to_string(&path).with_context(|| "Reading network config")?;
        generate_config(hostname, &data)?;
    }

    Ok(())
}

// Parse a YAML-based network configuration to the respective
// network configuration files per interface (*.nmconnection)
// and store those in the destination `hostname` directory.
fn generate_config(hostname: &str, data: &str) -> Result<(), anyhow::Error> {
    let network_state = NetworkState::new_from_yaml(data)?;

    let interfaces = extract_host_interfaces(hostname.to_string(), &network_state);
    let nm_config = network_state.gen_conf()?;

    store_network_config(hostname, &interfaces, &nm_config).with_context(|| "Storing config")?;

    Ok(())
}

fn extract_host_interfaces(hostname: String, network_state: &NetworkState) -> Vec<HostInterfaces> {
    let interfaces = network_state
        .interfaces
        .iter()
        .filter(|i| i.iface_type() != InterfaceType::Loopback)
        .filter(|i| i.base_iface().mac_address.is_some())
        .map(|i| Interface {
            logical_name: i.name().to_string(),
            mac_address: i.base_iface().mac_address.clone().unwrap(),
        })
        .collect();

    vec![HostInterfaces {
        hostname,
        interfaces,
    }]
}

fn store_network_config(
    hostname: &str,
    interfaces: &[HostInterfaces],
    nm_config: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), anyhow::Error> {
    fs::create_dir(hostname).with_context(|| "Creating output dir")?;

    let mapping_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(HOST_MAPPING_FILE)?;

    serde_yaml::to_writer(mapping_file, interfaces)?;

    nm_config
        .get("NetworkManager")
        .ok_or_else(|| anyhow!("Invalid NM configuration"))?
        .iter()
        .try_for_each(|(filename, content)| {
            let path = Path::new(hostname).join(filename);

            fs::write(path, content).with_context(|| "Writing config file")
        })?;

    Ok(())
}
