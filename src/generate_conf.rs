use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};
use log::{info, warn};
use nmstate::{InterfaceType, NetworkState};

use crate::types::{Host, Interface};
use crate::HOST_MAPPING_FILE;

/// NetworkConfig contains the generated configurations in the
/// following format: Vec<(config_file_name, config_content>)
type NetworkConfig = Vec<(String, String)>;

/// Generate network configurations from all YAML files in the `config_dir`
/// and store the result *.nmconnection files and host mapping under `output_dir`.
pub(crate) fn generate(config_dir: &str, output_dir: &str) -> Result<(), anyhow::Error> {
    for entry in fs::read_dir(config_dir)? {
        let entry = entry?;
        let path = entry.path();

        if entry.metadata()?.is_dir() {
            warn!("Ignoring unexpected dir: {path:?}");
            continue;
        }

        info!("Generating config from {path:?}...");

        let hostname = extract_host_name(&path)
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("Invalid file path"))?
            .to_string();

        let data = fs::read_to_string(&path).context("Reading network config")?;

        let (interfaces, config) = generate_config(data)?;

        store_network_config(output_dir, hostname, interfaces, config).context("Storing config")?;
    }

    Ok(())
}

fn extract_host_name(path: &Path) -> Option<&OsStr> {
    if path
        .extension()
        .is_some_and(|ext| ext == "yml" || ext == "yaml")
    {
        path.file_stem()
    } else {
        path.file_name()
    }
}

fn generate_config(data: String) -> Result<(Vec<Interface>, NetworkConfig), anyhow::Error> {
    let network_state = NetworkState::new_from_yaml(&data)?;

    let interfaces = extract_interfaces(&network_state);
    let config = network_state
        .gen_conf()?
        .get("NetworkManager")
        .ok_or_else(|| anyhow!("Invalid NM configuration"))?
        .to_owned();

    Ok((interfaces, config))
}

fn extract_interfaces(network_state: &NetworkState) -> Vec<Interface> {
    network_state
        .interfaces
        .iter()
        .filter(|i| i.iface_type() != InterfaceType::Loopback)
        .filter(|i| i.base_iface().mac_address.is_some())
        .map(|i| Interface {
            logical_name: i.name().to_string(),
            mac_address: i.base_iface().mac_address.clone().unwrap(),
        })
        .collect()
}

fn store_network_config(
    output_dir: &str,
    hostname: String,
    interfaces: Vec<Interface>,
    config: NetworkConfig,
) -> Result<(), anyhow::Error> {
    let path = Path::new(output_dir);

    fs::create_dir_all(path.join(&hostname)).context("Creating output dir")?;

    config.iter().try_for_each(|(filename, content)| {
        let path = path.join(&hostname).join(filename);

        fs::write(path, content).context("Writing config file")
    })?;

    let mapping_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.join(HOST_MAPPING_FILE))?;

    let hosts = [Host {
        hostname,
        interfaces,
    }];

    serde_yaml::to_writer(mapping_file, &hosts).context("Writing mapping file")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use crate::generate_conf::{extract_interfaces, generate, generate_config};
    use crate::types::{Host, Interface};
    use crate::HOST_MAPPING_FILE;

    #[test]
    fn generate_successfully() -> Result<(), anyhow::Error> {
        let config_dir = "testdata/generate";
        let exp_output_path = Path::new("testdata/generate/expected");
        let out_dir = "_out";
        let output_path = Path::new("_out").join("node1");

        assert!(generate(config_dir, out_dir).is_ok());

        // verify contents of *.nmconnection files
        let exp_eth0_conn = fs::read_to_string(exp_output_path.join("eth0.nmconnection"))?;
        let exp_bridge_conn = fs::read_to_string(exp_output_path.join("bridge0.nmconnection"))?;
        let exp_lo_conn = fs::read_to_string(exp_output_path.join("lo.nmconnection"))?;
        let eth0_conn = fs::read_to_string(output_path.join("eth0.nmconnection"))?;
        let bridge_conn = fs::read_to_string(output_path.join("bridge0.nmconnection"))?;
        let lo_conn = fs::read_to_string(output_path.join("lo.nmconnection"))?;

        assert_eq!(exp_eth0_conn, eth0_conn);
        assert_eq!(exp_bridge_conn, bridge_conn);
        assert_eq!(exp_lo_conn, lo_conn);

        // verify contents of the host mapping file
        let mut exp_hosts: Vec<Host> = serde_yaml::from_str(
            fs::read_to_string(exp_output_path.join(HOST_MAPPING_FILE))?.as_str(),
        )?;
        let mut hosts: Vec<Host> = serde_yaml::from_str(
            fs::read_to_string(Path::new(out_dir).join(HOST_MAPPING_FILE))?.as_str(),
        )?;

        assert_eq!(exp_hosts.len(), hosts.len());

        exp_hosts.sort_by(|a, b| a.hostname.cmp(&b.hostname));
        hosts.sort_by(|a, b| a.hostname.cmp(&b.hostname));

        for (h1, h2) in exp_hosts.iter_mut().zip(hosts.iter_mut()) {
            h1.interfaces
                .sort_by(|a, b| a.logical_name.cmp(&b.logical_name));
            h2.interfaces
                .sort_by(|a, b| a.logical_name.cmp(&b.logical_name));
        }

        assert_eq!(exp_hosts, hosts);

        // cleanup
        fs::remove_dir_all(out_dir)?;

        Ok(())
    }

    #[test]
    fn generate_fails_due_to_missing_path() {
        let error = generate("<missing>", "_out").unwrap_err();
        assert!(error.to_string().contains("No such file or directory"))
    }

    #[test]
    fn generate_config_fails_due_to_invalid_data() {
        let err = generate_config("<invalid>".to_string()).unwrap_err();
        assert!(err.to_string().contains("Invalid YAML string"))
    }

    #[test]
    fn extract_interfaces_skips_loopback() -> Result<(), serde_yaml::Error> {
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

        let mut interfaces = extract_interfaces(&net_state);
        interfaces.sort_by(|a, b| a.logical_name.cmp(&b.logical_name));

        assert_eq!(
            interfaces,
            vec![
                Interface {
                    logical_name: "bridge0".to_string(),
                    mac_address: "FE:C4:05:42:8B:AB".to_string(),
                },
                Interface {
                    logical_name: "eth1".to_string(),
                    mac_address: "FE:C4:05:42:8B:AA".to_string(),
                },
            ]
        );

        Ok(())
    }
}
