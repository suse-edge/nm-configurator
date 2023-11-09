use log::{error, info};

use apply_conf::apply;
use generate_conf::generate;

mod apply_conf;
mod generate_conf;
mod types;

const APP_NAME: &str = "nmc";

const SUB_CMD_GENERATE: &str = "generate";
const SUB_CMD_APPLY: &str = "apply";

/// Destination directory to store the *.nmconnection files for NetworkManager.
const SYSTEM_CONNECTIONS_DIR: &str = "/etc/NetworkManager/system-connections";

/// File storing a mapping between host identifier (usually hostname) and its preconfigured network interfaces.
const HOST_MAPPING_FILE: &str = "host_config.yaml";

fn main() {
    env_logger::init();

    let app = clap::Command::new(APP_NAME)
        .version(clap::crate_version!())
        .about("Command line of NM configurator")
        .subcommand_required(true)
        .subcommand(
            clap::Command::new(SUB_CMD_GENERATE)
                .about("Generate network configuration using nmstate")
                .arg(
                    clap::Arg::new("CONFIG-DIR")
                        .required(true)
                        .long("config-dir")
                        .help("Config dir containing network configurations for different hosts in YAML format"),
                )
                .arg(
                    clap::Arg::new("OUTPUT-DIR")
                        .default_value("_out")
                        .long("output-dir")
                        .help("Destination dir storing the output configurations"),
                ))
        .subcommand(
            clap::Command::new(SUB_CMD_APPLY)
                .about("Apply network configurations to host")
                .arg(
                    clap::Arg::new("CONFIG-DIR")
                        .long("config-dir")
                        .default_value("config")
                        .help("Config dir containing host mapping ('host_config.yaml') \
                         and subdirectories containing *.nmconnection files per host")
                )
        );

    let matches = app.get_matches();

    match matches.subcommand() {
        Some((SUB_CMD_GENERATE, cmd)) => {
            let config_dir = cmd
                .get_one::<String>("CONFIG-DIR")
                .expect("--config-dir is required");
            let output_dir = cmd
                .get_one::<String>("OUTPUT-DIR")
                .expect("--output-dir is required");

            match generate(config_dir, output_dir) {
                Ok(..) => {
                    info!("Successfully generated and stored network config");
                }
                Err(err) => {
                    error!("Generating config failed: {err:#}");
                    std::process::exit(1)
                }
            }
        }
        Some((SUB_CMD_APPLY, cmd)) => {
            let config_dir = cmd
                .get_one::<String>("CONFIG-DIR")
                .expect("--config-dir is required");

            match apply(config_dir, SYSTEM_CONNECTIONS_DIR) {
                Ok(..) => {
                    info!("Successfully applied config");
                }
                Err(err) => {
                    error!("Applying config failed: {err:#}");
                    std::process::exit(1)
                }
            }
        }
        _ => unreachable!("Unrecognized subcommand"),
    }
}
