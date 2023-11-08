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
