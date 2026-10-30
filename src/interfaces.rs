use std::fs;
use std::path::Path;

use anyhow::{anyhow, Context};

const SYSFS_NET_DIR: &str = "/sys/class/net";
const PERM_HWADDR_REL: &str = "bonding_slave/perm_hwaddr";
const ADDRESS_REL: &str = "address";
const IFINDEX_REL: &str = "ifindex";

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Clone))]
pub(crate) struct LocalInterface {
    pub(crate) name: String,
    pub(crate) mac_address: Option<String>,
}

pub(crate) fn list_local_interfaces() -> Result<Vec<LocalInterface>, anyhow::Error> {
    list_interfaces_in(SYSFS_NET_DIR)
}

fn list_interfaces_in<P: AsRef<Path>>(dir: P) -> Result<Vec<LocalInterface>, anyhow::Error> {
    let dir = dir.as_ref();
    let mut interfaces: Vec<(u32, LocalInterface)> = Vec::new();

    for entry in fs::read_dir(dir).with_context(|| format!("Reading {}", dir.display()))? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|n| anyhow!("Non-UTF8 interface name: {n:?}"))?;
        let path = entry.path();

        let ifindex = read_ifindex(&path).unwrap_or(u32::MAX);
        let mac_address = read_mac_address(&path)?;

        interfaces.push((ifindex, LocalInterface { name, mac_address }));
    }

    interfaces.sort_by_key(|(idx, _)| *idx);
    Ok(interfaces.into_iter().map(|(_, iface)| iface).collect())
}

fn read_ifindex(iface_path: &Path) -> Result<u32, anyhow::Error> {
    let path = iface_path.join(IFINDEX_REL);
    let raw = fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
    raw.trim()
        .parse::<u32>()
        .with_context(|| format!("Parsing ifindex from {}", path.display()))
}

/// Reads the MAC address of a network interface from sysfs.
///
/// For bond-enslaved interfaces the regular `address` file reports the bond's
/// MAC, masking the original. The permanent hardware address is preserved at
/// `bonding_slave/perm_hwaddr` and is preferred when available.
fn read_mac_address(iface_path: &Path) -> Result<Option<String>, anyhow::Error> {
    let perm_hwaddr = iface_path.join(PERM_HWADDR_REL);
    let path = if perm_hwaddr.exists() {
        perm_hwaddr
    } else {
        iface_path.join(ADDRESS_REL)
    };

    if !path.exists() {
        return Ok(None);
    }

    let mac = fs::read_to_string(&path)
        .with_context(|| format!("Reading {}", path.display()))?
        .trim()
        .to_string();

    if mac.is_empty() {
        Ok(None)
    } else {
        Ok(Some(mac))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::interfaces::{list_interfaces_in, LocalInterface};

    fn temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_iface(
        root: &Path,
        name: &str,
        ifindex: u32,
        address: &str,
        perm_hwaddr: Option<&str>,
    ) {
        let dir = root.join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ifindex"), format!("{ifindex}\n")).unwrap();
        fs::write(dir.join("address"), format!("{address}\n")).unwrap();
        if let Some(perm) = perm_hwaddr {
            let slave_dir = dir.join("bonding_slave");
            fs::create_dir_all(&slave_dir).unwrap();
            fs::write(slave_dir.join("perm_hwaddr"), format!("{perm}\n")).unwrap();
        }
    }

    #[test]
    fn lists_interfaces_sorted_by_ifindex() {
        let root = temp_dir("interfaces-sort");

        write_iface(&root, "lo", 1, "00:00:00:00:00:00", None);
        write_iface(&root, "bond99", 4, "34:8a:b1:4b:16:e7", None);
        write_iface(
            &root,
            "eth0",
            2,
            "34:8a:b1:4b:16:e7",
            Some("34:8a:b1:4b:16:e7"),
        );
        write_iface(
            &root,
            "eth1",
            3,
            "34:8a:b1:4b:16:e7",
            Some("34:8a:b1:4b:16:e8"),
        );

        let interfaces = list_interfaces_in(&root).unwrap();
        assert_eq!(
            interfaces,
            vec![
                LocalInterface {
                    name: "lo".to_string(),
                    mac_address: Some("00:00:00:00:00:00".to_string()),
                },
                LocalInterface {
                    name: "eth0".to_string(),
                    mac_address: Some("34:8a:b1:4b:16:e7".to_string()),
                },
                LocalInterface {
                    name: "eth1".to_string(),
                    mac_address: Some("34:8a:b1:4b:16:e8".to_string()),
                },
                LocalInterface {
                    name: "bond99".to_string(),
                    mac_address: Some("34:8a:b1:4b:16:e7".to_string()),
                },
            ]
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_address_file_yields_none() {
        let root = temp_dir("interfaces-missing-addr");

        let dir = root.join("eth0");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ifindex"), "2\n").unwrap();

        let interfaces = list_interfaces_in(&root).unwrap();
        assert_eq!(
            interfaces,
            vec![LocalInterface {
                name: "eth0".to_string(),
                mac_address: None,
            }]
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn empty_address_yields_none() {
        let root = temp_dir("interfaces-empty-addr");

        let dir = root.join("eth0");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("ifindex"), "2\n").unwrap();
        fs::write(dir.join("address"), "\n").unwrap();

        let interfaces = list_interfaces_in(&root).unwrap();
        assert_eq!(
            interfaces,
            vec![LocalInterface {
                name: "eth0".to_string(),
                mac_address: None,
            }]
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn missing_root_dir_returns_error() {
        assert!(list_interfaces_in("/does/not/exist/sysfs-net").is_err());
    }
}
