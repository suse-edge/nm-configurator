use std::ffi::OsStr;
use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};
use log::{debug, info, warn};
use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use nmstate::InterfaceType;

use crate::types::Host;
use crate::HOST_MAPPING_FILE;

const CONNECTION_FILE_EXT: &str = "nmconnection";
const HOSTNAME_FILE: &str = "/etc/hostname";

pub(crate) fn apply(source_dir: &str, destination_dir: &str) -> Result<(), anyhow::Error> {
    let hosts = parse_config(source_dir).context("Parsing config")?;
    debug!("Loaded hosts config: {hosts:?}");

    let network_interfaces = NetworkInterface::show()?;
    debug!("Retrieved network interfaces: {network_interfaces:?}");

    let host = identify_host(hosts, &network_interfaces)
        .ok_or_else(|| anyhow!("None of the preconfigured hosts match local NICs"))?;
    info!("Identified host: {}", host.hostname);

    fs::write(HOSTNAME_FILE, &host.hostname).context("Setting hostname")?;

    copy_connection_files(host, &network_interfaces, source_dir, destination_dir)
}

fn parse_config(source_dir: &str) -> Result<Vec<Host>, anyhow::Error> {
    let config_file = Path::new(source_dir).join(HOST_MAPPING_FILE);

    let file = fs::File::open(config_file)?;
    let mut hosts: Vec<Host> = serde_yaml::from_reader(file)?;

    // Ensure lower case formatting.
    hosts.iter_mut().for_each(|h| {
        h.interfaces.iter_mut().for_each(|i| match &i.mac_address {
            None => {}
            Some(addr) => i.mac_address = Some(addr.to_lowercase()),
        });
    });

    Ok(hosts)
}

/// Identify the preconfigured static host by matching the MAC address of at least one of the local network interfaces.
fn identify_host(hosts: Vec<Host>, network_interfaces: &[NetworkInterface]) -> Option<Host> {
    hosts.into_iter().find(|h| {
        h.interfaces.iter().any(|interface| {
            network_interfaces
                .iter()
                .filter(|nic| nic.mac_addr.is_some())
                .any(|nic| nic.mac_addr == interface.mac_address)
        })
    })
}

/// Copy all *.nmconnection files from the preconfigured host dir to the
/// appropriate NetworkManager dir (default `/etc/NetworkManager/system-connections`).
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
            .filter(|interface| interface.mac_address.is_some())
            .filter(|interface| interface.interface_type != InterfaceType::Vlan.to_string())
            .find(|interface| interface.logical_name == filename)
            .and_then(|interface| {
                network_interfaces
                    .iter()
                    .find(|nic| {
                        nic.mac_addr == interface.mac_address && nic.name != interface.logical_name
                    })
                    .filter(|nic| {
                        host.interfaces
                            .iter()
                            .find(|i| i.logical_name == nic.name)
                            .filter(|i| i.interface_type == InterfaceType::Vlan.to_string())
                            .is_none()
                    })
                    .map(|nic| (interface, &nic.name))
            })
        {
            info!("Using name '{}' for interface with MAC address '{:?}' instead of the preconfigured '{}'",
                nic_name, interface.mac_address, interface.logical_name);

            contents = contents.replace(&interface.logical_name, nic_name);
            filename = nic_name;
        }

        let destination = keyfile_destination_path(destination_dir, filename)
            .ok_or_else(|| anyhow!("Failed to determine destination path for: '{}'", filename))?;

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

fn keyfile_destination_path(dir: &str, filename: &str) -> Option<PathBuf> {
    if dir.is_empty() || filename.is_empty() {
        return None;
    }

    let mut destination = Path::new(dir).join(filename).into_os_string();

    // Manually append the extension since Path::with_extension() would overwrite a portion of the
    // filename (i.e. interface name) in the cases where the interface name contains one or more dots
    destination.push(".");
    destination.push(CONNECTION_FILE_EXT);

    Some(destination.into())
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    use network_interface::NetworkInterface;

    use crate::apply_conf::{
        copy_connection_files, identify_host, keyfile_destination_path, parse_config,
    };
    use crate::types::{Host, Interface};

    #[test]
    fn identify_host_successfully() {
        let hosts = vec![
            Host {
                hostname: "h1".to_string(),
                interfaces: vec![Interface {
                    logical_name: "eth0".to_string(),
                    mac_address: Option::from("00:11:22:33:44:55".to_string()),
                    interface_type: "ethernet".to_string(),
                }],
            },
            Host {
                hostname: "h2".to_string(),
                interfaces: vec![Interface {
                    logical_name: "".to_string(),
                    mac_address: Option::from("10:10:10:10:10:10".to_string()),
                    interface_type: "".to_string(),
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
                mac_address: Option::from("00:11:22:33:44:55".to_string()),
                interface_type: "ethernet".to_string(),
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
                    mac_address: Option::from("10:20:30:40:50:60".to_string()),
                    interface_type: "ethernet".to_string(),
                }],
            },
            Host {
                hostname: "h2".to_string(),
                interfaces: vec![Interface {
                    logical_name: "".to_string(),
                    mac_address: Option::from("00:10:20:30:40:50".to_string()),
                    interface_type: "".to_string(),
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
        let error = parse_config("<missing>").unwrap_err();
        assert!(error.to_string().contains("No such file or directory"))
    }

    #[test]
    fn parse_config_successfully() {
        let hosts = parse_config("testdata/apply/config").unwrap();
        assert_eq!(
            hosts,
            vec![
                Host {
                    hostname: "node1".to_string(),
                    interfaces: vec![
                        Interface {
                            logical_name: "eth0".to_string(),
                            mac_address: Option::from("00:11:22:33:44:55".to_string()),
                            interface_type: "ethernet".to_string(),
                        },
                        Interface {
                            logical_name: "eth1".to_string(),
                            mac_address: Option::from("00:11:22:33:44:58".to_string()),
                            interface_type: "ethernet".to_string(),
                        },
                        Interface {
                            logical_name: "eth2".to_string(),
                            mac_address: Option::from("36:5e:6b:a2:ed:80".to_string()),
                            interface_type: "ethernet".to_string(),
                        },
                        Interface {
                            logical_name: "bond0".to_string(),
                            mac_address: Option::from("00:11:22:aa:44:58".to_string()),
                            interface_type: "bond".to_string(),
                        },
                    ],
                },
                Host {
                    hostname: "node2".to_string(),
                    interfaces: vec![
                        Interface {
                            logical_name: "eth0".to_string(),
                            mac_address: Option::from("36:5e:6b:a2:ed:81".to_string()),
                            interface_type: "ethernet".to_string(),
                        },
                        Interface {
                            logical_name: "eth0.1365".to_string(),
                            mac_address: None,
                            interface_type: "vlan".to_string(),
                        },
                    ],
                },
            ]
        )
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
                    mac_address: Option::from("00:11:22:33:44:55".to_string()),
                    interface_type: "ethernet".to_string(),
                },
                Interface {
                    logical_name: "eth0.1365".to_string(),
                    mac_address: None,
                    interface_type: "vlan".to_string(),
                },
                Interface {
                    logical_name: "eth2".to_string(),
                    mac_address: Option::from("00:11:22:33:44:56".to_string()),
                    interface_type: "ethernet".to_string(),
                },
                Interface {
                    logical_name: "eth1".to_string(),
                    mac_address: Option::from("00:11:22:33:44:57".to_string()),
                    interface_type: "ethernet".to_string(),
                },
                Interface {
                    logical_name: "bond0".to_string(),
                    mac_address: Option::from("00:11:22:33:44:58".to_string()),
                    interface_type: "bond".to_string(),
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
                name: "eth0.1365".to_string(), // VLAN
                addr: vec![],
                mac_addr: Some("00:11:22:33:44:55".to_string()),
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
        for entry in fs::read_dir(source_path)? {
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

    #[test]
    fn generate_keyfile_destination_path() {
        assert_eq!(
            keyfile_destination_path("some-dir", "eth0"),
            Some(PathBuf::from("some-dir/eth0.nmconnection"))
        );
        assert_eq!(
            keyfile_destination_path("some-dir", "eth0.1234"),
            Some(PathBuf::from("some-dir/eth0.1234.nmconnection"))
        );
        assert!(keyfile_destination_path("some-dir", "").is_none());
        assert!(keyfile_destination_path("", "eth0").is_none());
    }
}
