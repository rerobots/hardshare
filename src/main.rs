// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

extern crate tokio;

#[macro_use]
extern crate clap;
use clap::{Arg, SubCommand};

mod api;
mod mgmt;


struct CliError {
    msg: Option<String>,
    exitcode: i32,
}
impl std::error::Error for CliError {}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.msg {
            Some(m) => write!(f, "{}", m),
            None => write!(f, "")
        }
    }
}

impl std::fmt::Debug for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.msg {
            Some(m) => write!(f, "{}", m),
            None => write!(f, "")
        }
    }
}

impl CliError {
    fn new(msg: &str, exitcode: i32) -> Result<(), CliError> {
        Err(CliError { msg: Some(String::from(msg)), exitcode: exitcode })
    }

    fn new_std(err: Box<dyn std::error::Error>, exitcode: i32) -> Result<(), CliError> {
        Err(CliError { msg: Some(format!("{}", err)), exitcode: exitcode })
    }
}


fn main() {
    match main_cli() {
        Ok(_) => std::process::exit(0),
        Err(err) => {
            if err.msg.is_some() {
                eprintln!("{}", err);
            }
            std::process::exit(err.exitcode);
        }
    }
}


fn main_cli() -> Result<(), CliError> {
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
    } else if let Some(matches) = matches.subcommand_matches("config") {
        let create_if_missing = matches.is_present("create_config");
        if matches.is_present("list") {

            let local_config = match mgmt::get_local_config(create_if_missing, true) {
                Ok(lc) => lc,
                Err(err) => return CliError::new_std(err, 1)
            };
            println!("{:?}", local_config);
            let ac = api::HSAPIClient::new();
            println!("{:?}", ac.get_remote_config(false));

        } else if create_if_missing {
            if let Err(err) = mgmt::get_local_config(true, false) {
                return CliError::new_std(err, 1);
            }
        } else {
            let errmessage = "Use `hardshare config` with a switch. For example, `hardshare config -l`\nor to get a help message, enter\n\n    hardshare help config";
            return CliError::new(errmessage, 1);
        }
    } else {
        println!("No command given. Try `hardshare -h`");
    }

    Ok(())
}
