use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};
use log::{info, warn};
use nmstate::{InterfaceType, NetworkState};
use serde::{Deserialize, Serialize};

const HOST_MAPPING_FILE: &str = "host_config.yaml";

#[derive(Serialize, Deserialize)]
pub struct HostInterfaces {
    hostname: String,
    interfaces: Vec<Interface>,
}

#[derive(Serialize, Deserialize)]
pub struct Interface {
    logical_name: String,
    mac_address: String,
}

pub(crate) fn generate(config_dir: &str, output_dir: &str) -> Result<(), anyhow::Error> {
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
        generate_config(hostname, &data, output_dir)?;
    }

    Ok(())
}

// Parse a YAML-based network configuration to the respective
// network configuration files per interface (*.nmconnection)
// and store those in the destination `hostname` directory.
fn generate_config(hostname: &str, data: &str, output_dir: &str) -> Result<(), anyhow::Error> {
    let network_state = NetworkState::new_from_yaml(data)?;

    let interfaces = extract_host_interfaces(hostname.to_string(), &network_state);
    let nm_config = network_state.gen_conf()?;

    store_network_config(output_dir, hostname, &interfaces, &nm_config)
        .with_context(|| "Storing config")?;

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
    output_dir: &str,
    hostname: &str,
    interfaces: &[HostInterfaces],
    nm_config: &HashMap<String, Vec<(String, String)>>,
) -> Result<(), anyhow::Error> {
    let path = Path::new(output_dir);

    fs::create_dir_all(path.join(hostname)).with_context(|| "Creating output dir")?;

    let mapping_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.join(HOST_MAPPING_FILE))?;

    serde_yaml::to_writer(mapping_file, interfaces)?;

    nm_config
        .get("NetworkManager")
        .ok_or_else(|| anyhow!("Invalid NM configuration"))?
        .iter()
        .try_for_each(|(filename, content)| {
            let path = path.join(hostname).join(filename);

            fs::write(path, content).with_context(|| "Writing config file")
        })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::generate_conf::{
        extract_host_interfaces, generate, generate_config, HostInterfaces, HOST_MAPPING_FILE,
    };

    #[test]
    fn generate_successfully() -> Result<(), anyhow::Error> {
        let config_dir = "testdata/generate";
        let exp_output_path = Path::new("testdata/generate/expected");
        let out_dir = "_out";
        let out_path = Path::new("_out").join("node1");

        assert_eq!(generate(config_dir, out_dir).is_ok(), true);

        let exp_hosts: Vec<HostInterfaces> = serde_yaml::from_str(
            fs::read_to_string(exp_output_path.join(HOST_MAPPING_FILE))?.as_str(),
        )?;
        let hosts: Vec<HostInterfaces> = serde_yaml::from_str(
            fs::read_to_string(Path::new(out_dir).join(HOST_MAPPING_FILE))?.as_str(),
        )?;

        assert_eq!(exp_hosts.len(), hosts.len());

        let exp_eth0_conn = fs::read_to_string(exp_output_path.join("eth0.nmconnection"))?;
        let exp_bridge_conn = fs::read_to_string(exp_output_path.join("bridge0.nmconnection"))?;
        let exp_lo_conn = fs::read_to_string(exp_output_path.join("lo.nmconnection"))?;
        let eth0_conn = fs::read_to_string(out_path.join("eth0.nmconnection"))?;
        let bridge_conn = fs::read_to_string(out_path.join("bridge0.nmconnection"))?;
        let lo_conn = fs::read_to_string(out_path.join("lo.nmconnection"))?;

        assert_eq!(exp_eth0_conn, eth0_conn);
        assert_eq!(exp_bridge_conn, bridge_conn);
        assert_eq!(exp_lo_conn, lo_conn);

        // cleanup
        fs::remove_dir_all(out_dir)?;

        Ok(())
    }

    #[test]
    fn generate_fails_due_to_missing_path() {
        let error = generate("<missing>", "_out").unwrap_err();
        assert_eq!(
            error.to_string().contains("No such file or directory"),
            true
        )
    }

    #[test]
    fn generate_config_fails_due_to_invalid_data() {
        let err = generate_config("host", "<invalid>", "_out").unwrap_err();
        assert_eq!(
            err.to_string()
                .contains("InvalidArgument: Invalid YAML string"),
            true
        )
    }

    #[test]
    fn extract_interfaces_skips_loopback() -> Result<(), serde_yaml::Error> {
        let hostname = String::from("host1");
        let net_state: nmstate::NetworkState = serde_yaml::from_str(
            r#"---
        interfaces:
          - name: eth1
            type: ethernet
            mac-address: FE:C4:05:42:8B:AA
          - name: bridge0
            type: linux-bridge
            mac-address: FE:C4:05:42:8B:AB
          - name: lo
            type: loopback
            mac-address: 00:00:00:00:00:00            
        "#,
        )?;

        let host_interfaces = extract_host_interfaces(hostname, &net_state);

        assert_eq!(host_interfaces.len(), 1);

        let host_interfaces = host_interfaces.get(0).unwrap();
        assert_eq!(host_interfaces.hostname, "host1");
        assert_eq!(host_interfaces.interfaces.len(), 2);

        let i1 = host_interfaces.interfaces.get(0).unwrap();
        let i2 = host_interfaces.interfaces.get(1).unwrap();

        let names = ["eth1".to_string(), "bridge0".to_string()];
        let addrs = [
            "FE:C4:05:42:8B:AA".to_string(),
            "FE:C4:05:42:8B:AB".to_string(),
        ];

        assert_eq!(names.contains(&i1.logical_name), true);
        assert_eq!(names.contains(&i2.logical_name), true);

        assert_eq!(addrs.contains(&i1.mac_address), true);
        assert_eq!(addrs.contains(&i2.mac_address), true);

        Ok(())
    }
}
