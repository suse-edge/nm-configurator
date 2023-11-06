use log::{error, info};

use generate_conf::generate;

mod generate_conf;

const APP_NAME: &str = "nmc";

const SUB_CMD_GENERATE: &str = "generate";

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
        _ => unreachable!("Unrecognized subcommand"),
    }
}