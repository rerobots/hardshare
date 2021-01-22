// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::io::prelude::*;

extern crate tokio;

use clap::{Arg, SubCommand};

use crate::{api, mgmt};


pub struct CliError {
    pub msg: Option<String>,
    pub exitcode: i32,
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

    fn new_stdio(err: std::io::Error, exitcode: i32) -> Result<(), CliError> {
        Err(CliError { msg: Some(format!("{}", err)), exitcode: exitcode })
    }

    fn newrc(exitcode: i32) -> Result<(), CliError> {
        Err(CliError { msg: None, exitcode: exitcode })
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
    if let Some(err_keys) = &local.err_keys {
        if err_keys.len() > 0 {
            println!("found possible keys with errors:");
        }
        for (err_key_path, err) in err_keys {
            println!("\t {}: {}", err, err_key_path);
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


fn config_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
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

    } else if let Some(new_token_path) = matches.value_of("new_api_token") {

        match mgmt::add_token_file(new_token_path) {
            Err(err) => return CliError::new_std(err, 1),
            Ok(_) => ()
        }

    } else if matches.is_present("prune_err_keys") {

        let local_config = match mgmt::get_local_config(false, true) {
            Ok(lc) => lc,
            Err(err) => return CliError::new_std(err, 1)
        };

        if let Some(err_keys) = &local_config.err_keys {
            for (err_key_path, _) in err_keys {
                match std::fs::remove_file(err_key_path) {
                    Err(err) => return CliError::new_stdio(err, 1),
                    Ok(_) => ()
                };
            }
        }

    } else if create_if_missing {
        if let Err(err) = mgmt::get_local_config(true, false) {
            return CliError::new_std(err, 1);
        }
    } else {
        let errmessage = "Use `hardshare config` with a switch. For example, `hardshare config -l`\nor to get a help message, enter\n\n    hardshare help config";
        return CliError::new(errmessage, 1);
    }

    Ok(())
}


fn rules_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1)
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1)
    };

    if matches.is_present("list_rules") {

        let ac = api::HSAPIClient::new();
        let mut ruleset = match ac.get_access_rules(local_config.wdeployments[wd_index]["id"].as_str().unwrap()) {
            Ok(r) => r,
            Err(err) => return CliError::new_std(err, 1)
        };

        if ruleset.comment.is_none() {
            ruleset.comment = Some("Access is denied unless a rule explicitly permits it.".into());
        }

        println!("{}", ruleset);

    } else if matches.is_present("drop_all_rules") {

        let ac = api::HSAPIClient::new();
        match ac.drop_access_rules(local_config.wdeployments[wd_index]["id"].as_str().unwrap()) {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1)
        }

    } else if matches.is_present("permit_me") {

        let ac = api::HSAPIClient::new();
        let wdid = local_config.wdeployments[wd_index]["id"].as_str().unwrap();
        let username = local_config.wdeployments[wd_index]["owner"].as_str().unwrap();
        match ac.add_access_rule(wdid, username) {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1)
        }

    } else if matches.is_present("permit_all") {

        let mut confirmation = String::new();
        loop {
            print!("Do you want to permit access by anyone? [y/N] ");
            std::io::stdout().flush().expect("failed to flush stdout");
            match std::io::stdin().read_line(&mut confirmation) {
                Ok(n) => n,
                Err(err) => return CliError::new_stdio(err, 1)
            };
            confirmation = confirmation.trim().to_lowercase();
            if confirmation == "y" || confirmation == "yes" {
                break;
            } else if confirmation.len() == 0 || confirmation == "n" || confirmation == "no" {
                return CliError::newrc(1);
            }
        }

        let ac = api::HSAPIClient::new();
        let wdid = local_config.wdeployments[wd_index]["id"].as_str().unwrap();
        match ac.add_access_rule(wdid, "*") {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1)
        }

    } else {
        return CliError::new("Use `hardshare rules` with a switch. For example, `hardshare rules -l`\nor to get a help message, enter\n\n    hardshare help rules", 1);
    }

    Ok(())
}


fn ad_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1)
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1)
    };

    let wdid = local_config.wdeployments[wd_index]["id"].as_str().unwrap();
    let ac = api::HSAPIClient::new();
    match ac.run(wdid, "127.0.0.1:6666") {
        Ok(()) => Ok(()),
        Err(err) => return CliError::new_std(err, 1)
    }
}


fn stop_ad_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let ac = api::HSAPIClient::new();
    match ac.stop("127.0.0.1:6666") {
        Ok(()) => Ok(()),
        Err(err) => return CliError::new_std(err, 1)
    }
}


fn register_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let mut ac = api::HSAPIClient::new();
    let at_most_1 = !matches.is_present("permit_more");
    match ac.register_new(at_most_1) {
        Ok(new_wdid) => {
            println!("{}", new_wdid);
            Ok(())
        },
        Err(err) => return CliError::new_std(err, 1)
    }
}


pub fn main() -> Result<(), CliError> {
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
                         .help("If no local configuration is found, then create one"))
                    .arg(Arg::with_name("new_api_token")
                         .long("add-key")
                         .value_name("FILE")
                         .help("add new API token"))
                    .arg(Arg::with_name("prune_err_keys")
                         .short("p")
                         .long("prune")
                         .help("delete files in local key directory that are not valid; to get list of files with errors, try `--list`")))
        .subcommand(SubCommand::with_name("ad")
                    .about("Advertise availability, accept new instances")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of workspace deployment to advertise (can be unique prefix); this argument is not required if there is only 1 workspace deployment")))
        .subcommand(SubCommand::with_name("rules")
                    .about("Modify access rules (also known as capabilities or permissions)")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of target workspace deployment (can be unique prefix); this argument is not required if there is only 1 workspace deployment"))
                    .arg(Arg::with_name("list_rules")
                         .short("l")
                         .long("list")
                         .help("list all rules"))
                    .arg(Arg::with_name("drop_all_rules")
                         .long("drop-all")
                         .help("Removes all access rules; note that access is denied by default, including to you (the owner)"))
                    .arg(Arg::with_name("permit_me")
                         .long("permit-me")
                         .help("Permit instantiations by you (the owner)"))
                    .arg(Arg::with_name("permit_all")
                         .long("permit-all")
                         .help("Permit instantiations by anyone")))
        .subcommand(SubCommand::with_name("stop-ad")
                    .about("Mark as unavailable; optionally wait for current instance to finish"))
        .subcommand(SubCommand::with_name("register")
                    .about("Register new workspace deployment")
                    .arg(Arg::with_name("permit_more")
                         .long("permit-more")
                         .help("Permits registration of more than 1 wdeployment; default is to fail if local configuration already has wdeployment declared")));

    let matches = app.get_matches();

    if matches.is_present("version") {
        println!(crate_version!());
    } else if let Some(_) = matches.subcommand_matches("version") {
        println!(crate_version!());
    } else if let Some(matches) = matches.subcommand_matches("config") {
        return config_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("rules") {
        return rules_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("ad") {
        return ad_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("stop-ad") {
        return stop_ad_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("register") {
        return register_subcommand(matches);
    } else {
        println!("No command given. Try `hardshare -h`");
    }

    Ok(())
}
