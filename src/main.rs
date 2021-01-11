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


fn print_config(local: &mgmt::Config, remote: &Option<serde_json::Value>) {
    println!("workspace deployments defined in local configuration:");
    if local.wdeployments.len() == 0 {
        println!("\t(none)");
    } else {
        for wd in local.wdeployments.iter() {
            println!("{}\n\turl: {}\n\towner: {}\n\tcprovider: {}\n\tcargs: {}",
                     wd["id"].as_str().unwrap(),
                     wd["url"].as_str().unwrap(),
                     wd["owner"].as_str().unwrap(),
                     wd["cprovider"].as_str().unwrap(),
                     wd["cargs"]);
            if wd["cprovider"] == "docker" || wd["cprovider"] == "podman" {
                println!("\timg: {}", wd["image"].as_str().unwrap());
            }
            if wd["terminate"].as_array().unwrap().len() > 0 {
                println!("\tterminate:");
                for terminate_p in wd["terminate"].as_array().unwrap().iter() {
                    println!("\t\t{}", terminate_p.as_str().unwrap());
                }
            }
        }
    }

    println!("\nfound keys:");
    if local.keys.len() == 0 {
        println!("\t(none)");
    } else {
        for k in local.keys.iter() {
            println!("\t{}", k);
        }
    }

    if let Some(remote_config) = remote {
        let rc_wds = &remote_config["deployments"].as_array().unwrap();
        if rc_wds.len() == 0 {
            println!("\nno registered workspace deployments with this user account");
        } else {
            println!("\nregistered workspace deployments with this user account:");
            for wd in rc_wds.iter() {
                println!("{}", wd["id"].as_str().unwrap());
                println!("\tcreated: {}", wd["date_created"].as_str().unwrap());
                if !wd["desc"].is_null() {
                    println!("\tdesc: {}", wd["desc"].as_str().unwrap());
                }
                let origin = if wd["origin"].is_null() {
                    "(unknown)"
                } else {
                    wd["origin"].as_str().unwrap()
                };
                println!("\torigin (address) of registration: {}", origin);
                if !wd["dissolved"].is_null() {
                    println!("\tdissolved: {}", wd["dissolved"].as_str().unwrap());
                }
            }
        }
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
                    .arg(Arg::with_name("onlylocalconfig")
                         .long("local")
                         .help("Only show local configuration data"))
                    .arg(Arg::with_name("includedissolved")
                         .long("--include-dissolved")
                         .help("Include configuration data of dissolved workspace deployments"))
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
        let only_local_config = matches.is_present("onlylocalconfig");
        let include_dissolved = matches.is_present("includedissolved");
        if matches.is_present("list") {

            let mut local_config = match mgmt::get_local_config(create_if_missing, true) {
                Ok(lc) => lc,
                Err(err) => return CliError::new_std(err, 1)
            };
            mgmt::append_urls(&mut local_config);

            let mut remote_config = None;
            if !only_local_config {
                let ac = api::HSAPIClient::new();
                remote_config = Some(match ac.get_remote_config(include_dissolved) {
                    Ok(rc) => rc,
                    Err(err) => return CliError::new(err.as_str(), 1)
                });
            }

            print_config(&local_config, &remote_config);

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
