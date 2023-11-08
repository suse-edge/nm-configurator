use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::{anyhow, Context};
use log::{debug, info, warn};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};

use crate::types::Host;
use crate::HOST_MAPPING_FILE;

const CONNECTION_FILE_EXT: &str = "nmconnection";

pub(crate) fn apply(source_dir: &str, destination_dir: &str) -> Result<(), anyhow::Error> {
    let hosts = parse_config(source_dir, HOST_MAPPING_FILE).context("Parsing config")?;
    debug!("Loaded hosts config: {hosts:?}");

    let network_interfaces = NetworkInterface::show()?;
    debug!("Retrieved network interfaces: {network_interfaces:?}");

    let host = identify_host(hosts, &network_interfaces)
        .ok_or_else(|| anyhow!("None of the preconfigured hosts match local NICs"))?;
    info!("Identified host: {}", host.hostname);

    copy_connection_files(host, &network_interfaces, source_dir, destination_dir)
}

fn parse_config(source_dir: &str, config_file_name: &str) -> Result<Vec<Host>, anyhow::Error> {
    let config_file = Path::new(source_dir).join(config_file_name);

    let file = fs::File::open(config_file)?;
    let mut hosts: Vec<Host> = serde_yaml::from_reader(file)?;

    // Ensure lower case formatting.
    hosts.iter_mut().for_each(|h| {
        h.interfaces
            .iter_mut()
            .for_each(|i| i.mac_address = i.mac_address.to_lowercase());
    });

    Ok(hosts)
}

/// Identify the preconfigured static host by matching the MAC address of at least one of the local network interfaces.
fn identify_host(hosts: Vec<Host>, network_interfaces: &[NetworkInterface]) -> Option<Host> {
    hosts.into_iter().find(|h| {
        h.interfaces.iter().any(|interface| {
            network_interfaces.iter().any(|nic| {
                nic.mac_addr
                    .clone()
                    .is_some_and(|addr| addr == interface.mac_address)
            })
        })
    })
}

/// Copy all *.nmconnection files from the preconfigured host dir to the
/// appropriate NetworkManager dir (default "/etc/NetworkManager/system-connections").
fn copy_connection_files(
    host: Host,
    network_interfaces: &[NetworkInterface],
    source_dir: &str,
    destination_dir: &str,
) -> Result<(), anyhow::Error> {
    fs::create_dir_all(destination_dir).context("Creating destination dir")?;

    let host_config_dir = Path::new(source_dir).join(&host.hostname);

    for entry in fs::read_dir(host_config_dir)? {
        let entry = entry?;
        let path = entry.path();

        if entry.metadata()?.is_dir()
            || path
                .extension()
                .and_then(OsStr::to_str)
                .unwrap_or_default()
                .ne(CONNECTION_FILE_EXT)
        {
            warn!("Ignoring unexpected entry: {path:?}");
            continue;
        }

        info!("Copying file... {path:?}");

        let mut contents = fs::read_to_string(&path).context("Reading file")?;

        let mut filename = path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("Invalid file path"))?;

        // Update the name and all references of the host NIC in the settings file if there is a difference from the static config.
        if let Some((interface, nic_name)) = host
            .interfaces
            .iter()
            .find(|interface| interface.logical_name == filename)
            .and_then(|interface| {
                network_interfaces
                    .iter()
                    .find(|nic| {
                        nic.mac_addr
                            .clone()
                            .is_some_and(|addr| addr == interface.mac_address)
                            && nic.name != interface.logical_name
                    })
                    .map(|nic| (interface, &nic.name))
            })
        {
            info!("Using name '{}' for interface with MAC address '{}' instead of the preconfigured '{}'",
                nic_name, interface.mac_address, interface.logical_name);

            contents = contents.replace(&interface.logical_name, nic_name);
            filename = nic_name;
        }

        let destination = Path::new(destination_dir)
            .join(filename)
            .with_extension(CONNECTION_FILE_EXT);

        fs::OpenOptions::new()
            .create(true)
            .write(true)
            .mode(0o600)
            .open(&destination)
            .context("Creating file")?
            .write_all(contents.as_bytes())
            .context("Writing file")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::{fs, io};

    use network_interface::NetworkInterface;

    use crate::apply_conf::{copy_connection_files, identify_host, parse_config};
    use crate::types::{Host, Interface};
    use crate::HOST_MAPPING_FILE;

    #[test]
    fn identify_host_successfully() {
        let hosts = vec![
            Host {
                hostname: "h1".to_string(),
                interfaces: vec![Interface {
                    logical_name: "eth0".to_string(),
                    mac_address: "00:11:22:33:44:55".to_string(),
                }],
            },
            Host {
                hostname: "h2".to_string(),
                interfaces: vec![Interface {
                    logical_name: "".to_string(),
                    mac_address: "10:10:10:10:10:10".to_string(),
                }],
            },
        ];
        let interfaces = [
            NetworkInterface {
                name: "eth0".to_string(),
                mac_addr: Some("00:11:22:33:44:55".to_string()),
                addr: vec![],
                index: 0,
            },
            NetworkInterface {
                name: "eth0".to_string(),
                mac_addr: Some("00:10:20:30:40:50".to_string()),
                addr: vec![],
                index: 0,
            },
        ];

        let host = identify_host(hosts, &interfaces).unwrap();
        assert_eq!(host.hostname, "h1");
        assert_eq!(
            host.interfaces,
            vec![Interface {
                logical_name: "eth0".to_string(),
                mac_address: "00:11:22:33:44:55".to_string(),
            }]
        );
    }

    #[test]
    fn identify_host_fails() {
        let hosts = vec![
            Host {
                hostname: "h1".to_string(),
                interfaces: vec![Interface {
                    logical_name: "eth0".to_string(),
                    mac_address: "10:20:30:40:50:60".to_string(),
                }],
            },
            Host {
                hostname: "h2".to_string(),
                interfaces: vec![Interface {
                    logical_name: "".to_string(),
                    mac_address: "00:10:20:30:40:50".to_string(),
                }],
            },
        ];
        let interfaces = [NetworkInterface {
            name: "eth0".to_string(),
            mac_addr: Some("00:11:22:33:44:55".to_string()),
            addr: vec![],
            index: 0,
        }];

        assert!(identify_host(hosts, &interfaces).is_none())
    }

    #[test]
    fn parse_config_fails_due_to_missing_file() {
        let error = parse_config("<missing-dir>", HOST_MAPPING_FILE).unwrap_err();
        assert!(error.to_string().contains("No such file or directory"))
    }

    #[test]
    fn copy_connection_files_successfully() -> io::Result<()> {
        let source_dir = "testdata/apply";
        let destination_dir = "_out";
        let host = Host {
            hostname: "node1".to_string(),
            interfaces: vec![
                Interface {
                    logical_name: "eth0".to_string(),
                    mac_address: "00:11:22:33:44:55".to_string(),
                },
                Interface {
                    logical_name: "eth2".to_string(),
                    mac_address: "00:11:22:33:44:56".to_string(),
                },
                Interface {
                    logical_name: "eth1".to_string(),
                    mac_address: "00:11:22:33:44:57".to_string(),
                },
                Interface {
                    logical_name: "bond0".to_string(),
                    mac_address: "00:11:22:33:44:58".to_string(),
                },
            ],
        };
        let interfaces = [
            NetworkInterface {
                name: "eth0".to_string(),
                mac_addr: Some("00:11:22:33:44:55".to_string()),
                addr: vec![],
                index: 0,
            },
            NetworkInterface {
                name: "eth4".to_string(),
                mac_addr: Some("00:11:22:33:44:56".to_string()),
                addr: vec![],
                index: 0,
            },
            NetworkInterface {
                name: "eth1".to_string(),
                mac_addr: Some("00:11:22:33:44:57".to_string()),
                addr: vec![],
                index: 0,
            },
            // NetworkInterface {
            //     name: "bond0", Excluded on purpose, "bond0.nmconnection" should still be copied
            // },
        ];

        assert!(copy_connection_files(host, &interfaces, source_dir, destination_dir).is_ok());

        let source_path = Path::new(source_dir).join("node1");
        let destination_path = Path::new(destination_dir);
        for entry in fs::read_dir(&source_path)? {
            let entry = entry?;

            let mut filename = entry.file_name().into_string().unwrap();
            let mut input = fs::read_to_string(entry.path())?;

            // Adjust the name and content for the "eth2"->"eth4" edge case.
            if entry.path().file_stem().is_some_and(|stem| stem == "eth2") {
                filename = filename.replace("eth2", "eth4");
                input = input.replace("eth2", "eth4");
            }

            let output = fs::read_to_string(destination_path.join(&filename))?;

            assert_eq!(input, output);
        }

        // cleanup
        fs::remove_dir_all(destination_dir)
    }
}
