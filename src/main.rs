// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

extern crate tokio;

#[macro_use]
extern crate clap;
use clap::{Arg, SubCommand};

mod api;
mod mgmt;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let app = clap::App::new("hardshare")
        .about("Command-line interface for the hardshare client")
        .subcommand(SubCommand::with_name("version")
                    .about("Prints version number and exits"))
        .arg(Arg::with_name("version")
             .short("V")
             .long("version")
             .help("Prints version number and exits"))
        .subcommand(SubCommand::with_name("config")
                    .about("Manage local and remote configuration")
                    .arg(Arg::with_name("list")
                         .short("l")
                         .long("list")
                         .help("Lists configuration"))
                    .arg(Arg::with_name("create_config")
                         .short("c")
                         .long("create")
                         .help("If no local configuration is found, then create one")));
    let matches = app.get_matches();

    if matches.is_present("version") {
        println!(crate_version!());
    } else if let Some(_) = matches.subcommand_matches("version") {
        println!(crate_version!());
    } if let Some(matches) = matches.subcommand_matches("config") {
        let create_if_missing = matches.is_present("create_config");
        if matches.is_present("list") {

            let local_config = mgmt::get_local_config(create_if_missing, true)?;
            println!("{:?}", local_config);
            let ac = api::HSAPIClient::new();
            println!("{:?}", ac.get_remote_config(false));

        } else if create_if_missing {
            mgmt::get_local_config(true, false)?;
        }
    } else {
        println!("No command given. Try `hardshare -h`");
    }

    Ok(())
}
