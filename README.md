# nm-configurator

nm-configurator (or NMC) is a CLI which makes it easy to generate and apply NetworkManager configurations.

## How to install it?

### Standard method

Each release is published with NMC already built for `amd64` and `arm64` Linux systems:

For AMD64 / x86_64 based systems:

```shell
$ curl -o nmc -L https://github.com/suse-edge/nm-configurator/releases/latest/download/nmc-x86_64 
$ chmod +x nmc
```

For ARM64 / aarch64 based systems:

```shell
$ curl -o nmc -L https://github.com/suse-edge/nm-configurator/releases/latest/download/nmc-aarch64
$ chmod +x nmc
```

### Manual method

```shell
$ git clone https://github.com/suse-edge/nm-configurator.git
$ cd nm-configurator
$ cargo build --release # optionally specify --target flag if cross compiling
```

## How to run it?

### Generate config

NMC depends on having the desired network state for all known nodes beforehand.

[NetworkManager](https://documentation.suse.com/sle-micro/5.5/html/SLE-Micro-all/cha-nm-configuration.html)
is using connection profiles defined as files stored under `/etc/NetworkManager/system-connections`.
In order to generate these config (*.nmconnection) files, NMC uses the
[nmstate](https://github.com/nmstate/nmstate) library and requires a configuration directory as an input.
This directory must contain the desired network state for all hosts in a <i>hostname</i>.yaml file format.
Please refer to the official nmstate docs for [examples](https://nmstate.io/examples.html).

```shell
$ export RUST_LOG=info # set log level ("error" by default)
$ ./nmc generate --config-dir config --output-dir _out
[2023-11-08T21:18:12Z INFO  nmc::generate_conf] Generating config from "config/node1.yaml"...
[2023-11-08T21:18:12Z INFO  nmc::generate_conf] Generating config from "config/node2.yaml"...
[2023-11-08T21:18:12Z INFO  nmc::generate_conf] Generating config from "config/node3.yaml"...
[2023-11-08T21:18:12Z INFO  nmc] Successfully generated and stored network config
```

The output is the following:

```shell
$ ls -R _out
_out:
host_config.yaml  node1  node2  node3

_out/node1:
bond0.nmconnection  eth0.nmconnection

_out/node2:
eth0.nmconnection  eth1.nmconnection

_out/node3:
eth2.nmconnection
```

There are separate directories for each host (identified by their input <i>hostname</i>.yaml).
Each of these contains the configuration files for the desired network interfaces (e.g. `eth0`).

The `host_config.yaml` file on the root level maps the hosts to all of their preconfigured interfaces.
This is necessary in order for NMC to identify which host it is running on when applying the network configurations later.

```yaml
- hostname: node1
  interfaces:
    - logical_name: eth0
      mac_address: FE:C4:05:42:8B:AA
    - logical_name: bond0
      mac_address: 0E:4D:C6:B8:C4:72
...
```

### Apply config

NMC will use the previously generated configurations to identify and store the relevant
NetworkManager settings for a given host out of the pool of predefined desired states.

Typically used with [Combustion](https://documentation.suse.com/sle-micro/5.5/single-html/SLE-Micro-deployment/#cha-images-combustion)
in order to bootstrap multiple nodes using the same provisioning artefact instead of depending on different custom images per machine.

```shell
$ export RUST_LOG=info # set log level ("error" by default)
$ ./nmc apply --config-dir _out/
[2023-11-08T22:24:52Z INFO  nmc::apply_conf] Identified host: node2
[2023-11-08T22:24:52Z INFO  nmc::apply_conf] Copying file... "_out/node2/eth0.nmconnection"
[2023-11-08T22:24:52Z INFO  nmc::apply_conf] Copying file... "_out/node2/eth1.nmconnection"
[2023-11-08T22:24:52Z INFO  nmc] Successfully applied config
```

**NOTE:** Interface names during the installation of nodes might differ from the preconfigured logical ones.
This is expected and NMC will rely on the MAC addresses and use the actual names for the NetworkManager
configurations instead e.g. settings for interface with a predefined logical name `eth0` but actually named
`eth2` will automatically be adjusted and stored to `/etc/NetworkManager/eth2.nmconnection`.
