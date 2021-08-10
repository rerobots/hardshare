// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::io::prelude::*;
use std::process::{Command, Stdio};

use serde::Serialize;

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
            None => write!(f, ""),
        }
    }
}

impl std::fmt::Debug for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.msg {
            Some(m) => write!(f, "{}", m),
            None => write!(f, ""),
        }
    }
}

impl CliError {
    fn new(msg: &str, exitcode: i32) -> Result<(), CliError> {
        Err(CliError {
            msg: Some(String::from(msg)),
            exitcode,
        })
    }

    fn new_std(err: Box<dyn std::error::Error>, exitcode: i32) -> Result<(), CliError> {
        Err(CliError {
            msg: Some(format!("{}", err)),
            exitcode,
        })
    }

    fn new_stdio(err: std::io::Error, exitcode: i32) -> Result<(), CliError> {
        Err(CliError {
            msg: Some(format!("{}", err)),
            exitcode,
        })
    }

    fn newrc(exitcode: i32) -> Result<(), CliError> {
        Err(CliError {
            msg: None,
            exitcode,
        })
    }
}


#[derive(PartialEq, Debug)]
enum PrintingFormat {
    DEFAULT,
    YAML,
    JSON,
}


fn print_config(
    local: &mgmt::Config,
    remote: &Option<serde_json::Value>,
    pformat: PrintingFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    print_config_w(&mut std::io::stdout(), local, remote, pformat)?;
    Ok(())
}


fn print_config_w<T: Write>(
    f: &mut T,
    local: &mgmt::Config,
    remote: &Option<serde_json::Value>,
    pformat: PrintingFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    if pformat != PrintingFormat::DEFAULT {
        fn serializer<T: Serialize>(x: &T, pformat: PrintingFormat) -> String {
            if pformat == PrintingFormat::JSON {
                serde_json::to_string(x).unwrap()
            } else {
                // if pformat == PrintingFormat::YAML
                serde_yaml::to_string(x).unwrap()
            }
        }

        if let Some(remote) = remote {
            let combined = json!({
                "local": local,
                "remote": remote
            });
            writeln!(f, "{}", serializer(&combined, pformat))?;
        } else {
            writeln!(f, "{}", serializer(&local, pformat))?;
        }
        return Ok(());
    }

    writeln!(f, "workspace deployments defined in local configuration:")?;
    if local.wdeployments.is_empty() {
        writeln!(f, "\t(none)")?;
    } else {
        for wd in local.wdeployments.iter() {
            writeln!(
                f,
                "{}\n\turl: {}\n\towner: {}\n\tcprovider: {}\n\tcargs: {}",
                wd["id"].as_str().unwrap(),
                wd["url"].as_str().unwrap(),
                wd["owner"].as_str().unwrap(),
                wd["cprovider"].as_str().unwrap(),
                wd["cargs"]
            )?;
            if wd["cprovider"] == "docker" || wd["cprovider"] == "podman" {
                writeln!(f, "\timg: {}", wd["image"].as_str().unwrap())?;
            }
            if !wd["terminate"].as_array().unwrap().is_empty() {
                writeln!(f, "\tterminate:")?;
                for terminate_p in wd["terminate"].as_array().unwrap().iter() {
                    writeln!(f, "\t\t{}", terminate_p.as_str().unwrap())?;
                }
            }
        }
    }

    writeln!(f, "\nfound API tokens:")?;
    if local.api_tokens.is_empty() {
        writeln!(f, "\t(none)")?;
    } else {
        for k in local.api_tokens.iter() {
            writeln!(f, "\t{}", k)?;
        }
    }
    if let Some(err_tokens) = &local.err_api_tokens {
        if !err_tokens.is_empty() {
            writeln!(f, "found possible API tokens with errors:")?;
        }
        for (err_token_path, err) in err_tokens {
            writeln!(f, "\t {}: {}", err, err_token_path)?;
        }
    }

    if let Some(remote_config) = remote {
        let rc_wds = &remote_config["deployments"].as_array().unwrap();
        if rc_wds.is_empty() {
            writeln!(
                f,
                "\nno registered workspace deployments with this user account"
            )?;
        } else {
            writeln!(
                f,
                "\nregistered workspace deployments with this user account:"
            )?;
            for wd in rc_wds.iter() {
                writeln!(f, "{}", wd["id"].as_str().unwrap())?;
                writeln!(f, "\tcreated: {}", wd["date_created"].as_str().unwrap())?;
                if !wd["desc"].is_null() {
                    writeln!(f, "\tdesc: {}", wd["desc"].as_str().unwrap())?;
                }
                let origin = if wd["origin"].is_null() {
                    "(unknown)"
                } else {
                    wd["origin"].as_str().unwrap()
                };
                writeln!(f, "\torigin (address) of registration: {}", origin)?;
                if !wd["dissolved"].is_null() {
                    writeln!(f, "\tdissolved: {}", wd["dissolved"].as_str().unwrap())?;
                }
            }
        }
    }

    Ok(())
}


fn config_subcommand(matches: &clap::ArgMatches, pformat: PrintingFormat) -> Result<(), CliError> {
    let create_if_missing = matches.is_present("create_config");
    let only_local_config = matches.is_present("onlylocalconfig");
    let include_dissolved = matches.is_present("includedissolved");

    if matches.is_present("list") {
        let mut local_config = match mgmt::get_local_config(create_if_missing, true) {
            Ok(lc) => lc,
            Err(err) => return CliError::new_std(err, 1),
        };
        mgmt::append_urls(&mut local_config);

        let mut remote_config = None;
        if !only_local_config {
            let ac = api::HSAPIClient::new();
            remote_config = Some(match ac.get_remote_config(include_dissolved) {
                Ok(rc) => rc,
                Err(err) => {
                    let err_message = format!("{}\nTo get only the local configuration, do\n\n    hardshare config -l --local", err);
                    return CliError::new(err_message.as_str(), 1);
                }
            });
        }

        print_config(&local_config, &remote_config, pformat).unwrap();
    } else if let Some(new_token_path) = matches.value_of("new_api_token") {
        if let Err(err) = mgmt::add_token_file(new_token_path) {
            return CliError::new_std(err, 1);
        }
    } else if matches.is_present("prune_err_tokens") {
        let local_config = match mgmt::get_local_config(false, true) {
            Ok(lc) => lc,
            Err(err) => return CliError::new_std(err, 1),
        };

        if let Some(err_tokens) = &local_config.err_api_tokens {
            for err_token_path in err_tokens.keys() {
                if let Err(err) = std::fs::remove_file(err_token_path) {
                    return CliError::new_stdio(err, 1);
                }
            }
        }
    } else if create_if_missing {
        if let Err(err) = mgmt::get_local_config(true, false) {
            return CliError::new_std(err, 1);
        }
    } else {
        // Remaining actions require a local configuration

        let mut local_config = match mgmt::get_local_config(false, false) {
            Ok(lc) => lc,
            Err(err) => return CliError::new_std(err, 1),
        };

        let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
            Ok(wi) => wi,
            Err(err) => return CliError::new_std(err, 1),
        };

        if let Some(cprovider) = matches.value_of("cprovider") {
            let selected_cprovider = cprovider.to_lowercase();
            if !vec!["docker", "podman", "proxy"].contains(&selected_cprovider.as_str()) {
                return CliError::new(
                    "cprovider must be one of the following: docker, podman, proxy",
                    1,
                );
            }

            if let Some(wd_cprovider) = local_config.wdeployments[wd_index].get_mut("cprovider") {
                let selected_cprovider = serde_json::Value::String(selected_cprovider);
                if *wd_cprovider == selected_cprovider {
                    return Ok(());
                }
                *wd_cprovider = selected_cprovider;
            } else {
                warn!(
                    "local configuration of {} without prior value of cprovider",
                    local_config.wdeployments[wd_index]["id"]
                );
                local_config.wdeployments[wd_index]
                    .insert("cprovider".into(), json!(selected_cprovider));
            }

            if local_config.wdeployments[wd_index]["cprovider"] == "proxy" {
                let null_img = json!(null);
                local_config.wdeployments[wd_index].insert("image".into(), null_img);
            } else {
                // cprovider \in {docker, podman}
                let default_img = json!("rerobots/hs-generic");
                if let Some(wd_cprovider_img) = local_config.wdeployments[wd_index].get_mut("image")
                {
                    if *wd_cprovider_img == json!(null) {
                        *wd_cprovider_img = default_img;
                    }
                } else {
                    local_config.wdeployments[wd_index].insert("image".into(), default_img);
                }
            }

            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(new_image) = matches.value_of("cprovider_img") {
            let cprovider = local_config.wdeployments[wd_index]["cprovider"]
                .as_str()
                .unwrap();
            match cprovider {
                "podman" => {
                    let argv = vec!["podman", "image", "exists", new_image];
                    let mut prog = Command::new(argv[0]);

                    debug!("exec: {:?}", argv);
                    let status = prog.args(&argv[1..]).status();
                    let status = match status {
                        Ok(s) => s,
                        Err(err) => return CliError::new_stdio(err, 1),
                    };
                    debug!("exit status: {:?}", status);

                    if !status.success() {
                        return CliError::new("given image name is not recognized by cprovider", 1);
                    }

                    let new_image = json!(new_image);
                    local_config.wdeployments[wd_index].insert("image".into(), new_image);
                }
                "docker" => {
                    let argv = vec!["docker", "image", "inspect", new_image];
                    let mut prog = Command::new(argv[0]);

                    debug!("exec: {:?}", argv);
                    let status = prog
                        .args(&argv[1..])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    let status = match status {
                        Ok(s) => s,
                        Err(err) => return CliError::new_stdio(err, 1),
                    };
                    debug!("exit status: {:?}", status);

                    if !status.success() {
                        return CliError::new("given image name is not recognized by cprovider", 1);
                    }

                    let new_image = json!(new_image);
                    local_config.wdeployments[wd_index].insert("image".into(), new_image);
                }
                _ => {
                    let errmessage = format!("cannot --assign-image for cprovider `{}`", cprovider);
                    return CliError::new(errmessage.as_str(), 1);
                }
            }

            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else {
            let errmessage = "Use `hardshare config` with a switch. For example, `hardshare config -l`\nor to get a help message, enter\n\n    hardshare help config";
            return CliError::new(errmessage, 1);
        }
    }

    Ok(())
}


fn config_addon_subcommand(
    matches: &clap::ArgMatches,
    pformat: PrintingFormat,
) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    let addon = match matches.value_of("addon") {
        Some("mistyproxy") => api::AddOn::MistyProxy,
        Some(_) => return CliError::new("unknown add-on", 1),
        _ => return CliError::new("add-on must be specified with `-a`", 1),
    };

    let ac = api::HSAPIClient::new();

    if matches.is_present("remove") {
        if let Err(err) = ac.remove_addon(
            local_config.wdeployments[wd_index]["id"].as_str().unwrap(),
            &addon,
        ) {
            return CliError::new_std(err, 1);
        }
    } else if matches.is_present("list") {
        let addon_config = match ac.get_addon_config(
            local_config.wdeployments[wd_index]["id"].as_str().unwrap(),
            &addon,
        ) {
            Ok(r) => r,
            Err(err) => return CliError::new_std(err, 1),
        };
        if pformat == PrintingFormat::JSON {
            println!("{}", serde_json::to_string(&addon_config).unwrap())
        } else {
            println!("{}", serde_yaml::to_string(&addon_config).unwrap())
        }
    } else if addon == api::AddOn::MistyProxy {
    }

    Ok(())
}


fn rules_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    if matches.is_present("list_rules") {
        let ac = api::HSAPIClient::new();
        let mut ruleset = match ac
            .get_access_rules(local_config.wdeployments[wd_index]["id"].as_str().unwrap())
        {
            Ok(r) => r,
            Err(err) => return CliError::new_std(err, 1),
        };

        if ruleset.comment.is_none() {
            ruleset.comment = Some("Access is denied unless a rule explicitly permits it.".into());
        }

        println!("{}", ruleset);
    } else if matches.is_present("drop_all_rules") {
        let ac = api::HSAPIClient::new();
        match ac.drop_access_rules(local_config.wdeployments[wd_index]["id"].as_str().unwrap()) {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1),
        }
    } else if matches.is_present("permit_me") {
        let ac = api::HSAPIClient::new();
        let wdid = local_config.wdeployments[wd_index]["id"].as_str().unwrap();
        let username = local_config.wdeployments[wd_index]["owner"]
            .as_str()
            .unwrap();
        match ac.add_access_rule(wdid, username) {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1),
        }
    } else if matches.is_present("permit_all") {
        let mut confirmation = String::new();
        loop {
            print!("Do you want to permit access by anyone? [y/N] ");
            std::io::stdout().flush().expect("failed to flush stdout");
            match std::io::stdin().read_line(&mut confirmation) {
                Ok(n) => n,
                Err(err) => return CliError::new_stdio(err, 1),
            };
            confirmation = confirmation.trim().to_lowercase();
            if confirmation == "y" || confirmation == "yes" {
                break;
            } else if confirmation.is_empty() || confirmation == "n" || confirmation == "no" {
                return CliError::newrc(1);
            }
        }

        let ac = api::HSAPIClient::new();
        let wdid = local_config.wdeployments[wd_index]["id"].as_str().unwrap();
        match ac.add_access_rule(wdid, "*") {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1),
        }
    } else {
        return CliError::new("Use `hardshare rules` with a switch. For example, `hardshare rules -l`\nor to get a help message, enter\n\n    hardshare help rules", 1);
    }

    Ok(())
}


fn ad_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wdid = local_config.wdeployments[wd_index]["id"].as_str().unwrap();
    let ac = api::HSAPIClient::new();
    match ac.run(wdid, "127.0.0.1:6666") {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}


fn stop_ad_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wdid = local_config.wdeployments[wd_index]["id"].as_str().unwrap();
    let ac = api::HSAPIClient::new();
    match ac.stop(wdid, "127.0.0.1:6666") {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}


fn register_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let mut ac = api::HSAPIClient::new();
    let at_most_1 = !matches.is_present("permit_more");
    match ac.register_new(at_most_1) {
        Ok(new_wdid) => {
            println!("{}", new_wdid);
            Ok(())
        }
        Err(err) => CliError::new_std(err, 1),
    }
}


pub fn main() -> Result<(), CliError> {
    let app = clap::App::new("hardshare")
        .max_term_width(80)
        .about("Command-line interface for the hardshare client")
        .subcommand(SubCommand::with_name("version")
                    .about("Prints version number and exits"))
        .arg(Arg::with_name("version")
             .short("V")
             .long("version")
             .help("Prints version number and exits"))
        .arg(Arg::with_name("verbose")
             .short("v")
             .long("verbose")
             .help("Increases verboseness level of logs; ignored if RUST_LOG is defined"))
        .arg(Arg::with_name("printformat")
             .long("format")
             .value_name("FORMAT")
             .help("special output formatting (default is no special formatting); options: YAML , JSON"))
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
                         .long("add-token")
                         .value_name("FILE")
                         .help("add new API token"))
                    .arg(Arg::with_name("prune_err_tokens")
                         .short("p")
                         .long("prune")
                         .help("delete files in local API tokens directory that are not valid; to get list of files with errors, try `--list`"))
                    .arg(Arg::with_name("cprovider")
                         .long("cprovider")
                         .value_name("CPROVIDER")
                         .help("select a container provider: docker, podman, proxy"))
                    .arg(Arg::with_name("cprovider_img")
                         .long("assign-image")
                         .value_name("IMG")
                         .help("assign image for cprovider to use (advanced option)"))
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of workspace deployment for configuration changes (can be unique prefix); this argument is not required if there is only 1 workspace deployment")))
        .subcommand(SubCommand::with_name("config-addon")
                    .about("Manage add-ons (mistyproxy, vnc, ...)")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of workspace deployment for add-ons (can be unique prefix)"))
                    .arg(Arg::with_name("addon")
                         .short("a")
                         .value_name("ADDON")
                         .help("name of the add-on"))
                    .arg(Arg::with_name("list")
                         .short("l")
                         .help("Lists configuration of add-on"))
                    .arg(Arg::with_name("ipv4")
                         .long("ip")
                         .value_name("ADDR")
                         .help("mistyproxy: IP address of Misty robot"))
                    .arg(Arg::with_name("remove")
                         .long("rm")
                         .help("remove add-on from workspace deployment; instances will not be able to use the add-on specified with `-a`")))
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
                    .about("Mark as unavailable; optionally wait for current instance to finish")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of workspace deployment to stop advertising (can be unique prefix); this argument is not required if there is only 1 workspace deployment")))
        .subcommand(SubCommand::with_name("register")
                    .about("Register new workspace deployment")
                    .arg(Arg::with_name("permit_more")
                         .long("permit-more")
                         .help("Permits registration of more than 1 wdeployment; default is to fail if local configuration already has wdeployment declared")));

    let matches = app.get_matches();

    let default_loglevel = if matches.is_present("verbose") {
        "info"
    } else {
        "warn"
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_loglevel))
        .init();

    let pformat = match matches.value_of("printformat") {
        Some(given_pformat) => {
            let given_pformat_lower = given_pformat.to_lowercase();
            if given_pformat_lower == "json" {
                PrintingFormat::JSON
            } else if given_pformat_lower == "yaml" {
                PrintingFormat::YAML
            } else {
                return CliError::new(
                    format!("unrecognized format: {}", given_pformat).as_str(),
                    1,
                );
            }
        }
        None => PrintingFormat::DEFAULT,
    };

    if matches.is_present("version") || matches.subcommand_matches("version").is_some() {
        println!(crate_version!());
    } else if let Some(matches) = matches.subcommand_matches("config") {
        return config_subcommand(matches, pformat);
    } else if let Some(matches) = matches.subcommand_matches("config-addon") {
        return config_addon_subcommand(matches, pformat);
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


#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::print_config_w;
    use super::PrintingFormat;
    use crate::mgmt;


    #[test]
    fn list_config_json() {
        let td = tempdir().unwrap();
        let base_path = td.path().join(".rerobots");
        let lconf = mgmt::get_local_config_bp(&base_path, true, false).unwrap();

        let mut buf: Vec<u8> = vec![];
        print_config_w(&mut buf, &lconf, &None, PrintingFormat::JSON).unwrap();
        let buf_parsing_result: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_slice(&buf);
        assert!(buf_parsing_result.is_ok());
    }
}
