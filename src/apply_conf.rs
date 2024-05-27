use std::collections::HashMap;
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
use crate::{ALL_HOSTS_DIR, HOST_MAPPING_FILE};

/// Destination directory to store the *.nmconnection files for NetworkManager.
const STATIC_SYSTEM_CONNECTIONS_DIR: &str = "/etc/NetworkManager/system-connections";
const RUNTIME_SYSTEM_CONNECTIONS_DIR: &str = "/var/run/NetworkManager/system-connections";
/// Configuration directory for NetworkManager options.
const CONFIG_DIR: &str = "/etc/NetworkManager/conf.d";
const CONNECTION_FILE_EXT: &str = "nmconnection";
const HOSTNAME_FILE: &str = "/etc/hostname";

pub(crate) fn apply(source_dir: &str) -> Result<(), anyhow::Error> {
    let unified_config_path = Path::new(source_dir).join(ALL_HOSTS_DIR);

    if unified_config_path.exists() {
        info!("Applying unified config...");
        copy_unified_connection_files(unified_config_path, STATIC_SYSTEM_CONNECTIONS_DIR)?;
    } else {
        let hosts = parse_hosts(source_dir).context("Parsing config")?;
        debug!("Loaded hosts config: {hosts:?}");

        let network_interfaces = NetworkInterface::show()?;
        debug!("Retrieved network interfaces: {network_interfaces:?}");

        let host = identify_host(hosts, &network_interfaces)
            .ok_or_else(|| anyhow!("None of the preconfigured hosts match local NICs"))?;
        info!("Identified host: {}", host.hostname);

        fs::write(HOSTNAME_FILE, &host.hostname).context("Setting hostname")?;
        info!("Set hostname: {}", host.hostname);

        let local_interfaces = detect_local_interfaces(&host, network_interfaces);
        copy_connection_files(
            host,
            local_interfaces,
            source_dir,
            STATIC_SYSTEM_CONNECTIONS_DIR,
        )
        .context("Copying connection files")?;
    }

    disable_wired_connections(CONFIG_DIR, RUNTIME_SYSTEM_CONNECTIONS_DIR)
        .context("Disabling wired connections")
}

fn parse_hosts(source_dir: &str) -> Result<Vec<Host>, anyhow::Error> {
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

/// Detect and return the differences between the preconfigured interfaces and their local representations.
///
/// Examples:
///     Desired Ethernet "eth0" -> Local "ens1f0"
///     Desired VLAN "eth0.1365" -> Local "ens1f0.1365"
fn detect_local_interfaces(
    host: &Host,
    network_interfaces: Vec<NetworkInterface>,
) -> HashMap<String, String> {
    let mut local_interfaces = HashMap::new();

    host.interfaces
        .iter()
        .filter(|interface| interface.interface_type == InterfaceType::Ethernet.to_string())
        .for_each(|interface| {
            let detected_interface = network_interfaces.iter().find(|nic| {
                nic.mac_addr == interface.mac_address
                    && !host.interfaces.iter().any(|i| i.logical_name == nic.name)
            });
            match detected_interface {
                None => {}
                Some(detected) => {
                    local_interfaces.insert(interface.logical_name.clone(), detected.name.clone());
                }
            };
        });

    // Look for non-Ethernet interfaces containing references to Ethernet ones differing from their preconfigured names.
    local_interfaces.clone().iter().for_each(|(key, value)| {
        host.interfaces
            .iter()
            .filter(|interface| {
                interface.logical_name.contains(key) && !interface.logical_name.eq(key)
            })
            .for_each(|interface| {
                let name = &interface.logical_name;
                local_interfaces.insert(name.clone(), name.replace(key, value));
            })
    });

    local_interfaces
}

/// Copy all *.nmconnection files from the preconfigured host dir to the
/// appropriate NetworkManager dir (default `/etc/NetworkManager/system-connections`).
fn copy_unified_connection_files(
    source_dir: PathBuf,
    destination_dir: &str,
) -> Result<(), anyhow::Error> {
    fs::create_dir_all(destination_dir).context("Creating destination dir")?;

    for entry in fs::read_dir(source_dir)? {
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

        let contents = fs::read_to_string(&path).context("Reading file")?;

        let filename = path
            .file_stem()
            .and_then(OsStr::to_str)
            .ok_or_else(|| anyhow!("Invalid file path"))?;

        store_connection_file(filename, contents, destination_dir).context("Storing file")?;
    }

    Ok(())
}

/// Copy all *.nmconnection files from the preconfigured host dir to the
/// appropriate NetworkManager dir (default `/etc/NetworkManager/system-connections`)
/// applying interface naming adjustments if necessary.
fn copy_connection_files(
    host: Host,
    local_interfaces: HashMap<String, String>,
    source_dir: &str,
    destination_dir: &str,
) -> Result<(), anyhow::Error> {
    fs::create_dir_all(destination_dir).context("Creating destination dir")?;

    let host_config_dir = Path::new(source_dir).join(&host.hostname);
    let host_config_dir = host_config_dir
        .to_str()
        .ok_or_else(|| anyhow!("Determining host config path"))?;

    for interface in &host.interfaces {
        info!("Processing interface '{}'...", &interface.logical_name);

        let mut filename = &interface.logical_name;

        let filepath = keyfile_path(host_config_dir, filename)
            .ok_or_else(|| anyhow!("Determining source keyfile path"))?;

        let mut contents = fs::read_to_string(filepath).context("Reading file")?;

        // Update the name and all references of the host NIC in the settings file if there is a difference from the static config.
        match local_interfaces.get(&interface.logical_name) {
            None => {}
            Some(local_name) => {
                info!(
                    "Using interface name '{}' instead of the preconfigured '{}'",
                    local_name, interface.logical_name
                );

                contents = contents.replace(&interface.logical_name, local_name);
                filename = local_name;
            }
        }

        store_connection_file(filename, contents, destination_dir).context("Storing file")?;
    }

    Ok(())
}

fn store_connection_file(
    filename: &str,
    contents: String,
    destination_dir: &str,
) -> Result<(), anyhow::Error> {
    let destination = keyfile_path(destination_dir, filename)
        .ok_or_else(|| anyhow!("Determining destination keyfile path"))?;

    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(destination)
        .context("Creating file")?
        .write_all(contents.as_bytes())
        .context("Writing file")
}

fn keyfile_path(dir: &str, filename: &str) -> Option<PathBuf> {
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

fn disable_wired_connections(config_dir: &str, conn_dir: &str) -> Result<(), anyhow::Error> {
    let _ = fs::remove_dir_all(conn_dir);
    fs::create_dir_all(conn_dir).context(format!("Recreating {} directory", conn_dir))?;

    fs::create_dir_all(config_dir).context(format!("Creating {} directory", config_dir))?;

    let config_path = Path::new(config_dir).join("no-auto-default.conf");
    let config_contents = "[main]\nno-auto-default=*\n";

    fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(config_path)
        .context("Creating config file")?
        .write_all(config_contents.as_bytes())
        .context("Writing config file")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    use network_interface::NetworkInterface;

    use crate::apply_conf::{
        copy_connection_files, copy_unified_connection_files, detect_local_interfaces,
        disable_wired_connections, identify_host, keyfile_path, parse_hosts,
    };
    use crate::types::{Host, Interface};

    #[test]
    fn disable_wired_conn() {
        assert!(disable_wired_connections("config", "connections").is_ok());

        assert!(Path::new("config").exists());
        assert!(Path::new("connections").exists());

        let config_contents = fs::read_to_string("config/no-auto-default.conf").unwrap();
        assert_eq!(config_contents, "[main]\nno-auto-default=*\n");

        // cleanup
        assert!(fs::remove_dir_all("config").is_ok());
        assert!(fs::remove_dir_all("connections").is_ok());
    }

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
    fn parse_hosts_fails_due_to_missing_file() {
        let error = parse_hosts("<missing>").unwrap_err();
        assert!(error.to_string().contains("No such file or directory"))
    }

    #[test]
    fn parse_hosts_successfully() {
        let hosts = parse_hosts("testdata/apply/config").unwrap();
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
    fn detect_interface_differences() {
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
                    logical_name: "eth2.bridge".to_string(),
                    mac_address: None,
                    interface_type: "linux-bridge".to_string(),
                },
                Interface {
                    logical_name: "bond0".to_string(),
                    mac_address: Option::from("00:11:22:33:44:58".to_string()),
                    interface_type: "bond".to_string(),
                },
            ],
        };
        let interfaces = vec![
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
                name: "ens1f0".to_string(),
                mac_addr: Some("00:11:22:33:44:56".to_string()),
                addr: vec![],
                index: 0,
            },
        ];

        let local_interfaces = detect_local_interfaces(&host, interfaces);
        assert_eq!(
            local_interfaces,
            HashMap::from([
                ("eth2".to_string(), "ens1f0".to_string()),
                ("eth2.bridge".to_string(), "ens1f0.bridge".to_string())
            ])
        )
    }

    #[test]
    fn copy_unified_connection_files_successfully() -> io::Result<()> {
        let source_dir = "testdata/apply/node1";
        let destination_dir = "_all-out";

        assert!(copy_unified_connection_files(source_dir.into(), destination_dir).is_ok());

        let destination_path = Path::new(destination_dir);
        for entry in fs::read_dir(source_dir)? {
            let entry = entry?;
            let filename = entry.file_name().into_string().unwrap();

            let input = fs::read_to_string(entry.path())?;
            let output = fs::read_to_string(destination_path.join(&filename))?;

            assert_eq!(input, output);
        }

        // cleanup
        fs::remove_dir_all(destination_dir)
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
        let detected_interfaces = HashMap::from([("eth2".to_string(), "eth4".to_string())]);

        assert!(
            copy_connection_files(host, detected_interfaces, source_dir, destination_dir).is_ok()
        );

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
    fn generate_keyfile_path() {
        assert_eq!(
            keyfile_path("some-dir", "eth0"),
            Some(PathBuf::from("some-dir/eth0.nmconnection"))
        );
        assert_eq!(
            keyfile_path("some-dir", "eth0.1234"),
            Some(PathBuf::from("some-dir/eth0.1234.nmconnection"))
        );
        assert!(keyfile_path("some-dir", "").is_none());
        assert!(keyfile_path("", "eth0").is_none());
    }
}
