# nm-configurator

nm-configurator (or nmc) is a CLI tool which makes it easy to generate and apply NetworkManager configurations.

## How to install it?

### Download from release

Each release is published with nmc already built for `amd64` and `arm64` Linux systems:

```shell
$ curl -o nmc -L https://github.com/suse-edge/nm-configurator/releases/latest/download/nmc-linux-$(uname -m)
$ chmod +x nmc
```

### Build from source

```shell
$ git clone https://github.com/suse-edge/nm-configurator.git
$ cd nm-configurator
$ cargo build --release # only supports Linux based systems
```

## How to run it?

### Generate config

nmc depends on having the desired network state for all known nodes beforehand.

[NetworkManager](https://documentation.suse.com/sle-micro/5.5/html/SLE-Micro-all/cha-nm-configuration.html)
is using connection profiles defined as files stored under `/etc/NetworkManager/system-connections`.

In order to generate these config (*.nmconnection) files, nmc uses the
[nmstate](https://github.com/nmstate/nmstate) library and requires a configuration directory as an input.
This directory must contain the desired network state in one of the following file formats:

1. A single or multiple `<hostname>.yaml` files containing the different configurations per host.
This method requires specifying the MAC addresses of all Ethernet interfaces for each host
in order to properly identify the relevant configurations when applying those.

2. A single `_all.yaml` file containing the configurations to be applied to all hosts.
This method does not depend on MAC address matching.

#### Prepare desired states

```shell
mkdir "desired-states"

cat <<- EOF > desired-states/node1.yaml
interfaces:
- name: eth0
  type: ethernet
  state: up
  mac-address: FE:C4:05:42:8B:AA
  ipv4:
    address:
    - ip: 192.168.122.250
      prefix-length: 24
    enabled: true
  ipv6:
    address:
    - ip: 2001:db8::1:1
      prefix-length: 64
    enabled: true
EOF

cat <<- EOF > desired-states/node2.yaml
interfaces:
- name: eth1
  type: ethernet
  state: up
  mac-address: FE:C4:05:42:8B:AB
  ipv4:
    address:
    - ip: 192.168.123.250
      prefix-length: 24
    enabled: true
  ipv6:
    enabled: false
EOF

cat <<- EOF > desired-states/node3.yaml
interfaces:
- name: eth4
  type: ethernet
  state: up
  mac-address: FE:C4:05:42:8B:AC
  ipv4:
    address:
    - ip: 192.168.124.250
      prefix-length: 24
    enabled: true
  ipv6:
    enabled: false
EOF
```

Please refer to the official nmstate docs for more extensive [examples](https://nmstate.io/examples.html).

#### Run nmc

```shell
$ ./nmc generate --config-dir desired-states --output-dir network-config
[2024-04-03T07:47:50Z INFO  nmc::generate_conf] Generating config from "desired-states/node1.yaml"...
[2024-04-03T07:47:50Z INFO  nmc::generate_conf] Generating config from "desired-states/node2.yaml"...
[2024-04-03T07:47:50Z INFO  nmc::generate_conf] Generating config from "desired-states/node3.yaml"...
[2024-04-03T07:47:50Z INFO  nmc] Successfully generated and stored network config
```

#### Examine results

The output is the following:

```shell
$ find network-config | sort
network-config
network-config/host_config.yaml
network-config/node1
network-config/node1/eth0.nmconnection
network-config/node2
network-config/node2/eth1.nmconnection
network-config/node3
network-config/node3/eth4.nmconnection
```

There are separate directories for each host (identified by their input <i>hostname</i>.yaml).
Each of these contains the configuration files for the desired network interfaces (e.g. `eth0`).

The `host_config.yaml` file on the root level maps the hosts to all of their preconfigured interfaces.
This is necessary in order for nmc to identify which host it is running on when applying the network configurations later.

```yaml
- hostname: node1
  interfaces:
    - logical_name: eth0
      mac_address: FE:C4:05:42:8B:AA
      interface_type: ethernet
- hostname: node2
  interfaces:
    - logical_name: eth1
      mac_address: FE:C4:05:42:8B:AB
      interface_type: ethernet
- hostname: node3
  interfaces:
    - logical_name: eth4
      mac_address: FE:C4:05:42:8B:AC
      interface_type: ethernet
```

**NOTE:** The `host_config.yaml` file will not be present if the input configurations are the same for all provisioned hosts (specified in `_all.yaml`).

### Apply config

nmc will use the previously generated configurations to identify and store the relevant NetworkManager settings for a given host.

Typically used with [Combustion](https://documentation.suse.com/sle-micro/5.5/single-html/SLE-Micro-deployment/#cha-images-combustion)
in order to bootstrap multiple nodes using the same provisioning artefact instead of depending on different custom images per machine.

#### Prepare network configurations

Simply copy the directory containing the results from `nmc generate` (`network-config` in the example above) to the target host.

#### Run NMC

```shell
$ ./nmc apply --config-dir network-config/
[2024-04-03T07:50:55Z INFO  nmc::apply_conf] Identified host: node2
[2024-04-03T07:50:55Z INFO  nmc::apply_conf] Processing interface 'eth1'...
[2024-04-03T07:50:55Z INFO  nmc] Successfully applied config
```

**NOTE:** Interface names during the installation of nodes might differ from the preconfigured logical ones.
This is expected and nmc will rely on the MAC addresses and use the actual names for the NetworkManager
configurations instead e.g. settings for interface with a predefined logical name `eth0` but actually named
`eth2` will automatically be adjusted and stored to `/etc/NetworkManager/eth2.nmconnection`.
