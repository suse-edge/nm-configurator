# nm-configurator

A tool capable of identifying & storing the relevant NetworkManager settings
for a given host out of a pool of predefined desired configurations.

Typically used with [Combustion](https://documentation.suse.com/sle-micro/5.4/single-html/SLE-Micro-deployment/#cha-images-combustion) 
in order to bootstrap multiple nodes using the same provisioning artefact instead of depending on different custom images per machine.

## What are the prerequisites?

### Desired network configurations per host

`nm-configurator` depends on having the desired network state for all known nodes beforehand.

[NetworkManager](https://documentation.suse.com/sle-micro/5.4/html/SLE-Micro-all/cha-nm-configuration.html) 
is using connection profiles defined as files stored under `/etc/NetworkManager/system-connections`.
These config files (*.nmconnection) can be generated using [nmstate](https://nmstate.io/features/gen_conf.html).

Each file contains the desired state for a single network interface (e.g. `eth0`).
Configurations for all interfaces for all known hosts must be generated using `nmstate`.

### Network interface mapping

Network interface mapping is required in order for `nm-configurator`
to identify the proper configurations for each host it is running on.

This additional config must be provided in a YAML format mapping the logical name of the interface to its MAC address:

```yaml
host_config:
  - hostname: node1.example.com
    interfaces:
      - logical_name: eth0
        mac_address: 00:10:20:30:40:50
      - logical_name: eth1
        mac_address: 10:20:30:40:50:60        
  - hostname: node2.example.com
    interfaces:
      - logical_name: eth0
        mac_address: 00:11:22:33:44:55
```

**NOTE:** Interface names during the installation of nodes might differ from the preconfigured logical ones.
This is expected and `nm-configurator` will rely on the MAC addresses and use the actual names for the
NetworkManager configurations instead e.g. settings for interface with a predefined logical name `eth0` but
actually named `eth0.101` will automatically be adjusted and stored to `/etc/NetworkManager/eth0.101.nmconnection`.

## How to install it?

### Standard method:

Each release is published with `nm-configurator` already built for `amd64` and `arm64` Linux systems:

For AMD64 / x86_64 based systems:
```shell
$ curl -o nm-configurator -L https://github.com/suse-edge/nm-configurator/releases/latest/download/nm-configurator-amd64 
$ chmod +x nm-configurator
```

For ARM64 based systems:
```shell
$ curl -o nm-configurator -L https://github.com/suse-edge/nm-configurator/releases/latest/download/nm-configurator-arm64 
$ chmod +x nm-configurator
```

### Manual method:

```shell
$ git clone https://github.com/suse-edge/nm-configurator.git
$ cd nm-configurator
$ go build . # optionally specify GOOS and GOARCH flags if cross compiling
```

## How to run it?

Using an example configuration of three known nodes (with hostnames `node1.example.com`, `node2.example.com`
and `node3.example.com` and their respective NetworkManager settings) and interface mapping defined in `host_config.yaml`:

```text
config
├── node1.example.com
│   ├── eth0.nmconnection
│   └── eth1.nmconnection
├── node2.example.com
│   └── eth0.nmconnection
├── node3.example.com
│   ├── bond0.nmconnection
│   └── eth1.nmconnection
└── host_config.yaml
```

```shell
$ ./nm-configurator -config-dir=config -hosts-config-file=host_config.yaml -verbose
DEBU[2023-08-17T17:32:23+03:00] storing file "./etc/NetworkManager/system-connections/eth0.nmconnection"... 
DEBU[2023-08-17T17:32:23+03:00] storing file "./etc/NetworkManager/system-connections/eth1.nmconnection"... 
INFO[2023-08-17T17:32:23+03:00] successfully configured network manager 
```

*Note:* The default values for `-config-dir` and `-hosts-config-file` flags are `config` and `host_config.yaml`
respectively so providing them is not necessary with the file structure in the example:

```shell
$ ./nm-configurator -verbose
DEBU[2023-08-17T17:45:41+03:00] storing file "./etc/NetworkManager/system-connections/eth0.nmconnection"... 
DEBU[2023-08-17T17:45:41+03:00] storing file "./etc/NetworkManager/system-connections/eth1.nmconnection"... 
INFO[2023-08-17T17:45:41+03:00] successfully configured network manager 
```
