// Copyright (C) 2020 rerobots, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::convert::TryFrom;
use std::io::prelude::*;
use std::process::{Command, Stdio};

use serde::Serialize;

use chrono::{TimeZone, Utc};

use clap::{Arg, SubCommand};

use rerobots::client::TokenClaims;

use crate::api::{CameraCrop, CameraDimensions};
use crate::camera;
use crate::mgmt::CProvider;
use crate::{api, check, mgmt, monitor};

pub struct CliError {
    pub msg: Option<String>,
    pub exitcode: i32,
}
impl std::error::Error for CliError {}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.msg {
            Some(m) => write!(f, "{m}"),
            None => write!(f, ""),
        }
    }
}

impl std::fmt::Debug for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.msg {
            Some(m) => write!(f, "{m}"),
            None => write!(f, ""),
        }
    }
}

impl From<Box<dyn std::error::Error>> for CliError {
    fn from(value: Box<dyn std::error::Error>) -> Self {
        let disp = format!("{value}");
        let msg = if disp.is_empty() { None } else { Some(disp) };
        CliError { msg, exitcode: 1 }
    }
}

impl CliError {
    fn new<S>(msg: S, exitcode: i32) -> Result<(), CliError>
    where
        S: ToString,
    {
        Err(CliError {
            msg: Some(msg.to_string()),
            exitcode,
        })
    }

    fn new_std(err: Box<dyn std::error::Error>, exitcode: i32) -> Result<(), CliError> {
        Err(CliError {
            msg: Some(format!("{err}")),
            exitcode,
        })
    }

    fn new_stdio(err: std::io::Error, exitcode: i32) -> Result<(), CliError> {
        Err(CliError {
            msg: Some(format!("{err}")),
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
    Default,
    Yaml,
    Json,
}

fn confirm(prompt: &str) -> Result<(), CliError> {
    let mut confirmation = String::new();
    loop {
        print!("{prompt}");
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
    Ok(())
}

fn print_config(
    local: &mgmt::Config,
    remote: &Option<serde_json::Value>,
    pformat: PrintingFormat,
    show_all_remote: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    print_config_w(
        &mut std::io::stdout(),
        local,
        remote,
        pformat,
        show_all_remote,
    )?;
    Ok(())
}

fn print_config_w<T: Write>(
    f: &mut T,
    local: &mgmt::Config,
    remote: &Option<serde_json::Value>,
    pformat: PrintingFormat,
    show_all_remote: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if pformat != PrintingFormat::Default {
        fn serializer<T: Serialize>(x: &T, pformat: PrintingFormat) -> String {
            if pformat == PrintingFormat::Json {
                serde_json::to_string(x).unwrap()
            } else {
                // if pformat == PrintingFormat::Yaml
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

    let mut local_ids = vec![];
    writeln!(f, "workspace deployments defined in local configuration:")?;
    if local.wdeployments.is_empty() {
        writeln!(f, "\t(none)")?;
    } else {
        for wd in local.wdeployments.iter() {
            local_ids.push(wd.id.as_str());
            writeln!(
                f,
                "{}\n\turl: {}\n\towner: {}\n\tcprovider: {}\n\tcargs: {}",
                wd.id,
                wd.url.clone().unwrap(),
                wd.owner,
                wd.cprovider,
                wd.cargs.join(", "),
            )?;
            if wd.cprovider == CProvider::Docker
                || wd.cprovider == CProvider::DockerRootless
                || wd.cprovider == CProvider::Podman
                || wd.cprovider == CProvider::Lxd
            {
                match &wd.image {
                    Some(img) => {
                        writeln!(f, "\timg: {img}")?;
                    }
                    None => {
                        writeln!(f, "\timg: (none)")?;
                    }
                }
            }
            if !wd.init_inside.is_empty() {
                writeln!(f, "\tinit inside:")?;
                for init_inside_p in wd.init_inside.iter() {
                    writeln!(f, "\t\t{init_inside_p}")?;
                }
            }
            if !wd.terminate.is_empty() {
                writeln!(f, "\tterminate:")?;
                for terminate_p in wd.terminate.iter() {
                    writeln!(f, "\t\t{terminate_p}")?;
                }
            }
            if let Some(m) = &wd.monitor {
                writeln!(f, "\tmonitor: {m}")?;
            }
        }
    }

    write!(f, "\ndefault org: ")?;
    match &local.default_org {
        Some(dorg) => writeln!(f, "{dorg}")?,
        None => writeln!(f, "(none)")?,
    };

    writeln!(f, "\nfound API tokens:")?;
    if local.api_tokens.is_empty() {
        writeln!(f, "\t(none)")?;
    } else {
        let print_keys = |f: &mut T,
                          indent_level: u8,
                          paths: &Vec<String>,
                          data: &Vec<TokenClaims>|
         -> Result<(), Box<dyn std::error::Error>> {
            let prefix = match indent_level {
                0 => "",
                _ => "\t",
            };
            let mut data_iter = data.iter();
            for path in paths.iter() {
                let data = data_iter.next().unwrap();
                writeln!(f, "{prefix}\t{path}")?;
                write!(f, "{prefix}\t\texpiration: ")?;
                match data.expiration {
                    Some(exp) => writeln!(f, "{}", Utc.timestamp_opt(exp as i64, 0).unwrap())?,
                    None => writeln!(f, "(none)")?,
                };
            }
            Ok(())
        };

        if local.api_tokens.contains_key("()") {
            print_keys(f, 0, &local.api_tokens["()"], &local.api_tokens_data["()"])?;
        }
        for (org, org_tokens) in local.api_tokens.iter() {
            if org == "()" {
                continue;
            }
            writeln!(f, "\t{org}:")?;
            print_keys(f, 1, org_tokens, &local.api_tokens_data[org])?;
        }
    }
    if let Some(err_tokens) = &local.err_api_tokens {
        if !err_tokens.is_empty() {
            writeln!(f, "found possible API tokens with errors:")?;
        }
        for (err_token_path, err) in err_tokens {
            writeln!(f, "\t {err}: {err_token_path}")?;
        }
    }

    if let Some(remote_config) = remote {
        let rc_wds = &remote_config["wdeployments"].as_array().unwrap();
        if rc_wds.is_empty() {
            writeln!(
                f,
                "\nno registered workspace deployments with this user account"
            )?;
        } else {
            if show_all_remote {
                writeln!(
                    f,
                    "\nregistration details for all workspace deployments owned by this user:"
                )?;
            } else {
                writeln!(
                    f,
                    "\nregistration details for workspace deployments in local config:"
                )?;
            }
            for wd in rc_wds.iter() {
                if !show_all_remote && !local_ids.contains(&wd["id"].as_str().unwrap()) {
                    continue;
                }
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
                writeln!(f, "\torigin (address) of registration: {origin}")?;
                if !wd["dissolved"].is_null() {
                    writeln!(f, "\tdissolved: {}", wd["dissolved"].as_str().unwrap())?;
                }
                let locked_out = wd["lockout"].as_bool().unwrap();
                if locked_out {
                    writeln!(f, "\tlock-out: {locked_out}")?;
                }
            }
        }
    }

    Ok(())
}

fn list_subcommand(matches: &clap::ArgMatches, pformat: PrintingFormat) -> Result<(), CliError> {
    let only_local_config = matches.is_present("onlylocalconfig");
    let include_dissolved = matches.is_present("includedissolved");
    let show_all_remote = matches.is_present("list_all");

    let mut local_config = match mgmt::get_local_config(false, true) {
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
                let err_message = format!(
                    "{err}\nTo get only the local configuration, do\n\n    hardshare list --local"
                );
                return CliError::new(err_message.as_str(), 1);
            }
        });
    }

    match print_config(&local_config, &remote_config, pformat, show_all_remote) {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}

fn config_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    if let Some(new_token_path) = matches.value_of("new_api_token") {
        let mut local_config = match mgmt::get_local_config(false, true) {
            Ok(lc) => lc,
            Err(err) => return CliError::new_std(err, 1),
        };
        match mgmt::add_token_file(new_token_path) {
            Ok(Some(org_name)) => {
                if !local_config.known_orgs.contains(&org_name) {
                    local_config.known_orgs.push(org_name);
                }
            }
            Ok(None) => {}
            Err(err) => return CliError::new_std(err, 1),
        }
        return match mgmt::modify_local(&local_config) {
            Err(err) => CliError::new_std(err, 1),
            Ok(()) => Ok(()),
        };
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
    } else if let Some(new_ssh_path) = matches.value_of("new_ssh_path") {
        match mgmt::add_ssh_path(new_ssh_path) {
            Ok(()) => {}
            Err(err) => return CliError::new_std(err, 1),
        }
    } else if let Some(declared_wdeployment_id) = matches.value_of("declare_wdeployment_id") {
        let mut ac = api::HSAPIClient::new();
        match ac.declare_existing(declared_wdeployment_id) {
            Ok(()) => {}
            Err(err) => return CliError::new_std(err, 1),
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
            let selected_cprovider = match CProvider::try_from(cprovider.to_lowercase().as_str()) {
                Ok(c) => c,
                Err(err) => {
                    return CliError::new(err, 1);
                }
            };

            if local_config.wdeployments[wd_index].cprovider == selected_cprovider {
                return Ok(());
            }

            if selected_cprovider == CProvider::Proxy {
                local_config.wdeployments[wd_index].image = None;
            } else {
                if local_config.wdeployments[wd_index].cprovider == CProvider::Proxy {
                    local_config.wdeployments[wd_index].cargs = vec![];
                }
                let default_img = "rerobots/hs-generic";
                if local_config.wdeployments[wd_index].image.is_none() {
                    local_config.wdeployments[wd_index].image = Some(default_img.into());
                }
            }
            local_config.wdeployments[wd_index].cprovider = selected_cprovider;

            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(new_image) = matches.value_of("cprovider_img") {
            match local_config.wdeployments[wd_index].cprovider {
                CProvider::Podman => {
                    let argv = vec!["podman", "image", "exists", new_image];
                    let mut prog = Command::new(argv[0]);

                    debug!("exec: {argv:?}");
                    let status = prog.args(&argv[1..]).status();
                    let status = match status {
                        Ok(s) => s,
                        Err(err) => return CliError::new_stdio(err, 1),
                    };
                    debug!("exit status: {status:?}");

                    if !status.success() {
                        return CliError::new("given image name is not recognized by cprovider", 1);
                    }
                }
                CProvider::Docker | CProvider::DockerRootless => {
                    let argv = vec!["docker", "image", "inspect", new_image];
                    let mut prog = Command::new(argv[0]);

                    debug!("exec: {argv:?}");
                    let status = prog
                        .args(&argv[1..])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    let status = match status {
                        Ok(s) => s,
                        Err(err) => return CliError::new_stdio(err, 1),
                    };
                    debug!("exit status: {status:?}");

                    if !status.success() {
                        return CliError::new("given image name is not recognized by cprovider", 1);
                    }
                }
                CProvider::Lxd => {
                    let argv = vec!["lxc", "image", "show", new_image];
                    let mut prog = Command::new(argv[0]);

                    debug!("exec: {argv:?}");
                    let status = prog
                        .args(&argv[1..])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status();
                    let status = match status {
                        Ok(s) => s,
                        Err(err) => return CliError::new_stdio(err, 1),
                    };
                    debug!("exit status: {status:?}");

                    if !status.success() {
                        return CliError::new("given image name is not recognized by cprovider", 1);
                    }
                }
                _ => {
                    let errmessage = format!(
                        "cannot --assign-image for cprovider `{}`",
                        local_config.wdeployments[wd_index].cprovider
                    );
                    return CliError::new(errmessage.as_str(), 1);
                }
            }

            local_config.wdeployments[wd_index].image = Some(new_image.into());

            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(new_command) = matches.value_of("cprovider_cmd") {
            match local_config.wdeployments[wd_index].cprovider {
                CProvider::Proxy => {
                    let parts: Vec<&str> = new_command.split(' ').collect();
                    match parts[0] {
                        "rrhttp" => {
                            if parts.len() != 2 {
                                return CliError::new("Usage: rrhttp TARGET", 1);
                            }
                        }
                        _ => return CliError::new("unknown proxy command", 1),
                    };
                    local_config.wdeployments[wd_index].cargs =
                        parts.iter().map(|x| x.to_string()).collect();
                }
                _ => {
                    let errmessage = format!(
                        "cannot --assign-proxy-command for cprovider `{}`",
                        local_config.wdeployments[wd_index].cprovider
                    );
                    return CliError::new(errmessage.as_str(), 1);
                }
            }

            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(raw_device_path) = matches.value_of("raw_device_path") {
            let device_path = match std::path::Path::new(raw_device_path).canonicalize() {
                Ok(p) => p,
                Err(err) => return CliError::new_stdio(err, 1),
            };
            if !device_path.exists() {
                return CliError::new("device does not exist", 1);
            }
            let device_path = device_path.to_str().unwrap();
            if local_config.wdeployments[wd_index].cprovider == CProvider::Docker
                || local_config.wdeployments[wd_index].cprovider == CProvider::Podman
            {
                let new_carg = format!("--device={device_path}:{device_path}");
                if local_config.wdeployments[wd_index]
                    .cargs
                    .contains(&new_carg)
                {
                    return CliError::new("device already added", 1);
                }
                local_config.wdeployments[wd_index].cargs.push(new_carg);
            } else {
                return CliError::new("adding devices not supported by this cprovider", 1);
            }

            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(device_path) = matches.value_of("remove_raw_device_path") {
            if local_config.wdeployments[wd_index].cprovider == CProvider::Docker
                || local_config.wdeployments[wd_index].cprovider == CProvider::Podman
            {
                let mut carg = format!("--device={device_path}:{device_path}");
                if !local_config.wdeployments[wd_index].cargs.contains(&carg) {
                    let device_path_c = match std::path::Path::new(device_path).canonicalize() {
                        Ok(p) => p,
                        Err(err) => return CliError::new_stdio(err, 1),
                    };
                    let device_path_c = device_path_c.to_str().unwrap();
                    carg = format!("--device={device_path_c}:{device_path_c}");
                    if !local_config.wdeployments[wd_index].cargs.contains(&carg) {
                        return CliError::new("device not previously added", 1);
                    }
                }
                let index = local_config.wdeployments[wd_index]
                    .cargs
                    .iter()
                    .position(|x| x == &carg)
                    .unwrap();
                local_config.wdeployments[wd_index].cargs.remove(index);
            } else {
                return CliError::new("adding/removing devices not supported by this cprovider", 1);
            }

            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(program) = matches.value_of("add_terminate_prog") {
            local_config.wdeployments[wd_index]
                .terminate
                .push(program.into());
            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(program) = matches.value_of("rm_terminate_prog") {
            return match local_config.wdeployments[wd_index]
                .terminate
                .iter()
                .position(|x| x == program)
            {
                Some(index) => {
                    local_config.wdeployments[wd_index].terminate.remove(index);
                    match mgmt::modify_local(&local_config) {
                        Err(err) => CliError::new_std(err, 1),
                        Ok(()) => Ok(()),
                    }
                }
                None => CliError::new("no matching program found", 1),
            };
        } else if let Some(program) = matches.value_of("add_init_inside") {
            local_config.wdeployments[wd_index]
                .init_inside
                .push(program.into());
            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(program) = matches.value_of("rm_init_inside") {
            return match local_config.wdeployments[wd_index]
                .init_inside
                .iter()
                .position(|x| x == program)
            {
                Some(index) => {
                    local_config.wdeployments[wd_index]
                        .init_inside
                        .remove(index);
                    match mgmt::modify_local(&local_config) {
                        Err(err) => CliError::new_std(err, 1),
                        Ok(()) => Ok(()),
                    }
                }
                None => CliError::new("no matching program found", 1),
            };
        } else if let Some(program) = matches.value_of("monitor_prog") {
            if program == "-" {
                local_config.wdeployments[wd_index].monitor = None;
            } else {
                local_config.wdeployments[wd_index].monitor = Some(program.into());
            }
            return match mgmt::modify_local(&local_config) {
                Err(err) => CliError::new_std(err, 1),
                Ok(()) => Ok(()),
            };
        } else if let Some(raw_addr) = matches.value_of("hook_emails") {
            let addr = if raw_addr == "-" {
                vec![]
            } else {
                raw_addr.split(',').collect()
            };
            let ac = api::HSAPIClient::new();
            return match ac.register_hook_emails(&local_config.wdeployments[wd_index].id, addr) {
                Ok(()) => Ok(()),
                Err(err) => CliError::new_std(err, 1),
            };
        } else {
            let errmessage = "Use `hardshare config` with a switch. To get a help message, enter\n\n    hardshare help config";
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
    let wdid = &local_config.wdeployments[wd_index].id;

    if matches.is_present("remove") {
        if let Err(err) = ac.remove_addon(wdid, &addon) {
            return CliError::new_std(err, 1);
        }
    } else if matches.is_present("list") {
        let addon_config = match ac.get_addon_config(wdid, &addon) {
            Ok(r) => r,
            Err(err) => return CliError::new_std(err, 1),
        };
        if pformat == PrintingFormat::Json {
            println!("{}", serde_json::to_string(&addon_config).unwrap())
        } else {
            println!("{}", serde_yaml::to_string(&addon_config).unwrap())
        }
    } else if addon == api::AddOn::MistyProxy {
        if matches.is_present("ipv4") {
            if let Err(err) = ac.add_mistyproxy(wdid, matches.value_of("ipv4").unwrap()) {
                return CliError::new_std(err, 1);
            }
        } else {
            return CliError::new("No command. Try `hardshare help config-addon`", 1);
        }
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
        let mut ruleset = match ac.get_access_rules(&local_config.wdeployments[wd_index].id) {
            Ok(r) => r,
            Err(err) => return CliError::new_std(err, 1),
        };

        if ruleset.comment.is_none() {
            ruleset.comment = Some("Access is denied unless a rule explicitly permits it.".into());
        }

        println!("{ruleset}");
    } else if matches.is_present("drop_all_rules") {
        let ac = api::HSAPIClient::new();
        match ac.drop_access_rules(&local_config.wdeployments[wd_index].id) {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1),
        }
    } else if matches.is_present("permit_me") {
        let ac = api::HSAPIClient::new();
        let wdid = &local_config.wdeployments[wd_index].id;
        let username = &local_config.wdeployments[wd_index].owner;
        match ac.add_access_rule(wdid, username) {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1),
        }
    } else if matches.is_present("permit_all") {
        confirm("Do you want to permit access by anyone? [y/N] ")?;

        let ac = api::HSAPIClient::new();
        match ac.add_access_rule(&local_config.wdeployments[wd_index].id, "*") {
            Ok(_) => (),
            Err(err) => return CliError::new_std(err, 1),
        }
    } else {
        return CliError::new("Use `hardshare rules` with a switch. For example, `hardshare rules -l`\nor to get a help message, enter\n\n    hardshare help rules", 1);
    }

    Ok(())
}

fn ad_subcommand(matches: &clap::ArgMatches, bindaddr: &str) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    if local_config.wdeployments[wd_index].cprovider == CProvider::Proxy {
        if let Err(err) = check::check_proxy(&local_config.wdeployments[wd_index]) {
            return CliError::new(err, 1);
        }
    }

    let ac = api::HSAPIClient::new();
    match ac.run(&local_config.wdeployments[wd_index].id, bindaddr) {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}

fn stop_ad_subcommand(matches: &clap::ArgMatches, bindaddr: &str) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    let ac = api::HSAPIClient::new();
    match ac.stop(&local_config.wdeployments[wd_index].id, bindaddr) {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}

fn register_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let mut ac = api::HSAPIClient::new();
    let at_most_1 = !matches.is_present("permit_more");
    match ac.register_new(at_most_1) {
        Ok(new_wdid) => {
            println!("{new_wdid}");
            Ok(())
        }
        Err(err) => CliError::new_std(err, 1),
    }
}

fn declare_default_org_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let mut local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };
    let org_name = match matches.value_of("org_name") {
        Some(n) => {
            if n.is_empty() {
                None
            } else {
                let n = String::from(n);
                if !local_config.known_orgs.contains(&n) {
                    return CliError::new(format!("unknown organization \"{n}\"").as_str(), 1);
                }
                Some(n)
            }
        }
        None => None,
    };
    local_config.default_org = org_name;
    match mgmt::modify_local(&local_config) {
        Err(err) => CliError::new_std(err, 1),
        Ok(()) => Ok(()),
    }
}

fn lock_wdeplyoment_subcommand(
    matches: &clap::ArgMatches,
    make_locked: bool,
) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    let ac = api::HSAPIClient::new();
    match ac.toggle_lockout(&local_config.wdeployments[wd_index].id, make_locked) {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}

fn status_subcommand(bindaddr: &str, pformat: PrintingFormat) -> Result<(), CliError> {
    let ac = api::HSAPIClient::new();
    match ac.get_local_status(bindaddr) {
        Ok(r) => {
            if pformat == PrintingFormat::Json {
                println!("{}", serde_json::to_string(&r).unwrap());
            } else if pformat == PrintingFormat::Yaml {
                println!("{}", serde_yaml::to_string(&r).unwrap());
            } else {
                println!("{r}");
            }
            Ok(())
        }
        Err(err) => CliError::new(format!("{err}\nIs the local hardshare client active?"), 1),
    }
}

fn reload_subcommand(bindaddr: &str) -> Result<(), CliError> {
    let ac = api::HSAPIClient::new();
    match ac.req_reload_config(bindaddr) {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}

fn dissolve_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    confirm(&format!(
        "Do you want to dissolve {}? This action cannot be undone. [y/N] ",
        &local_config.wdeployments[wd_index].id,
    ))?;

    let mut ac = api::HSAPIClient::new();
    match ac.dissolve_wdeployment(&local_config.wdeployments[wd_index].id) {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}

fn attach_camera_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    return CliError::new("only Linux and Mac supported", 1);

    if matches.values_of("id_prefix").is_some()
        && matches.values_of("attach_camera_crop_config").is_some()
    {
        return CliError::new(
            "Give either ID arguments or crop configuration, but not both.",
            1,
        );
    }

    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let default_dev = camera::get_default_dev();
    let camera_path = matches.value_of("camera_path").unwrap_or(&default_dev);

    let mut wds = if matches.values_of("attach_camera_crop_config").is_none() {
        let wds = match matches.values_of("id_prefix") {
            Some(v) => v.collect(),
            None => vec![],
        };
        match mgmt::expand_id_prefixes(&local_config, &wds) {
            Ok(w) => w,
            Err(err) => return CliError::new_std(err, 1),
        }
    } else {
        vec![]
    };

    let width_height = match matches.value_of("attach_camera_res") {
        Some(wh) => match wh.parse::<CameraDimensions>() {
            Ok(c) => Some(c),
            Err(err) => return CliError::new(err, 1),
        },
        None => None,
    };

    let crop: Option<CameraCrop> = match matches.value_of("attach_camera_crop_config") {
        Some(c) => match serde_json::from_str::<CameraCrop>(c) {
            Ok(c) => {
                for crop_wd in c.keys() {
                    let mut matched = false;
                    for wd in local_config.wdeployments.iter() {
                        if &wd.id == crop_wd {
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        return CliError::new(
                            format!("unknown ID in crop configuration: {crop_wd}"),
                            1,
                        );
                    }
                    wds.push(crop_wd.clone());
                }
                Some(c)
            }
            Err(err) => {
                return CliError::new(format!("failed to parse crop configuration: {err}"), 1)
            }
        },
        None => None,
    };

    let ac = api::HSAPIClient::new();
    match ac.attach_camera(camera_path, &wds, &width_height, &crop) {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}

fn stop_cameras_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let all = matches.is_present("all_cameras");
    let force = matches.is_present("force");
    let ac = api::HSAPIClient::new();
    match ac.stop_cameras(all, force) {
        Ok(()) => Ok(()),
        Err(err) => CliError::new_std(err, 1),
    }
}

fn init_subcommand() -> Result<(), CliError> {
    if mgmt::get_local_config(false, false).is_ok() {
        return CliError::new("Cannot init: local configuration already exists", 1);
    }
    if let Err(err) = mgmt::get_local_config(true, false) {
        return CliError::new_std(err, 1);
    }

    Ok(())
}

fn check_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let mut at_least_one_error = false;

    let local_config = mgmt::get_local_config(false, true).ok();

    if let Some(config) = &local_config {
        if let Some(err_tokens) = &config.err_api_tokens {
            if !err_tokens.is_empty() {
                at_least_one_error = true;
                println!("found possible API tokens with errors:");
            }
            for (err_token_path, err) in err_tokens {
                println!("\t {err}: {err_token_path}");
            }
            if !err_tokens.is_empty() {
                println!("remove these with the following command:\n\n    hardshare config -p\n");
            }
        }
        if config.api_tokens.is_empty() {
            at_least_one_error = true;
            println!("no valid API tokens\nCreate and manage API tokens at\n\n    https://rerobots.net/tokens\n");
        } else {
            let org_name = match &config.default_org {
                Some(default_org) => default_org.as_str(),
                None => "()",
            };
            if !config.api_tokens.contains_key(org_name) || config.api_tokens[org_name].is_empty() {
                at_least_one_error = true;
                let suffix = if config.default_org.is_none() {
                    "".into()
                } else {
                    format!(": {org_name}")
                };
                println!("no valid API tokens for default org{suffix}");
            }
        }

        if at_least_one_error && matches.is_present("fail_fast") {
            return CliError::newrc(1);
        }
    }

    if matches.value_of("id_prefix").is_some() {
        if local_config.is_none() {
            return CliError::new("given ID when local configuration is undefined", 1);
        }
        let local_config = local_config.unwrap();

        let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
            Ok(wi) => wi,
            Err(err) => return CliError::new_std(err, 1),
        };

        match check::config(
            &local_config,
            matches.is_present("camera"),
            &local_config.wdeployments[wd_index].id,
            None,
            matches.is_present("fail_fast"),
        ) {
            Ok(()) => {
                if at_least_one_error {
                    CliError::newrc(1)
                } else {
                    Ok(())
                }
            }
            Err(err) => Err(err.into()),
        }
    } else if matches.is_present("all") {
        if local_config.is_none() {
            return CliError::new("local configuration is undefined", 1);
        }
        let local_config = local_config.unwrap();

        match check::all_configurations(
            &local_config,
            matches.is_present("camera"),
            matches.is_present("fail_fast"),
        ) {
            Ok(()) => {
                if at_least_one_error {
                    CliError::newrc(1)
                } else {
                    Ok(())
                }
            }
            Err(err) => Err(err.into()),
        }
    } else {
        match check::defaults(
            matches.is_present("camera"),
            matches.is_present("fail_fast"),
        ) {
            Ok(()) => {
                if at_least_one_error {
                    CliError::newrc(1)
                } else {
                    Ok(())
                }
            }
            Err(err) => {
                if local_config.is_some() {
                    let local_config = local_config.unwrap();
                    if !local_config.wdeployments.is_empty() {
                        println!("Note: Try `check` with a deployment ID or `--all` to check your deployments.\nNote: The following errors are from checking default settings.");
                    }
                }
                Err(err.into())
            }
        }
    }
}

fn monitor_subcommand(matches: &clap::ArgMatches) -> Result<(), CliError> {
    let local_config = match mgmt::get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return CliError::new_std(err, 1),
    };

    let wd_index = match mgmt::find_id_prefix(&local_config, matches.value_of("id_prefix")) {
        Ok(wi) => wi,
        Err(err) => return CliError::new_std(err, 1),
    };

    if matches.is_present("loop") {
        let duration = match matches.value_of("loop").unwrap().parse::<u64>() {
            Ok(d) => d,
            Err(err) => return CliError::new(err, 1),
        };
        match monitor::run_loop(
            &local_config,
            wd_index,
            std::time::Duration::from_secs(duration),
        ) {
            Ok(()) => Ok(()),
            Err(err) => CliError::new_std(err, 1),
        }
    } else {
        match monitor::run(&local_config, wd_index) {
            Ok(()) => Ok(()),
            Err(err) => CliError::new_std(err, 1),
        }
    }
}

pub fn main() -> Result<(), CliError> {
    let app = clap::App::new("hardshare")
        .bin_name("hardshare")
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
        .arg(Arg::with_name("daemonport")
             .long("port")
             .value_name("PORT")
             .help("port for daemon")
             .default_value("6666"))
        .subcommand(SubCommand::with_name("init")
                    .about("Initialize local configuration"))
        .subcommand(SubCommand::with_name("list")
                    .about("List configuration")
                    .arg(Arg::with_name("list_all")
                         .long("all")
                         .help("Lists all registered devices owned by this user, whether or not in the local configuration"))
                    .arg(Arg::with_name("onlylocalconfig")
                         .long("local")
                         .help("Only show local configuration data"))
                    .arg(Arg::with_name("includedissolved")
                         .long("--include-dissolved")
                         .help("Include configuration data of dissolved workspace deployments")))
        .subcommand(SubCommand::with_name("config")
                    .about("Manage local and remote configuration")
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
                         .help("select a container provider: lxd, docker, docker-rootless, podman, proxy"))
                    .arg(Arg::with_name("cprovider_img")
                         .long("assign-image")
                         .value_name("IMG")
                         .help("assign image for cprovider to use (advanced option)"))
                    .arg(Arg::with_name("cprovider_cmd")
                         .long("assign-proxy-command")
                         .value_name("CMD"))
                    .arg(Arg::with_name("raw_device_path")
                         .long("add-raw-device")
                         .value_name("PATH")
                         .help("add device file to present in container"))
                    .arg(Arg::with_name("remove_raw_device_path")
                         .long("rm-raw-device")
                         .value_name("PATH")
                         .help("remove device previously marked for inclusion in container"))
                    .arg(Arg::with_name("new_ssh_path")
                         .long("add-ssh-path")
                         .value_name("FILE")
                         .help("add path of SSH key pair (does not copy the key)"))
                    .arg(Arg::with_name("declare_wdeployment_id")
                         .long("declare")
                         .value_name("ID")
                         .help("declare that workspace deployment is hosted here. (This only works if it has been previously registered under the same user account.)"))
                    .arg(Arg::with_name("add_terminate_prog")
                         .long("add-terminate-prog")
                         .value_name("PROGRAM")
                         .help("add program to list of commands to execute"))
                    .arg(Arg::with_name("rm_terminate_prog")
                         .long("rm-terminate-prog")
                         .value_name("PROGRAM")
                         .help("remove program from list of commands to execute; for example, copy-and-paste value shown in `hardshare list` here"))
                    .arg(Arg::with_name("add_init_inside")
                         .long("add-init-inside")
                         .value_name("PROGRAM")
                         .help("add program to be executed inside container during initialization"))
                    .arg(Arg::with_name("rm_init_inside")
                         .long("rm-init-inside")
                         .value_name("PROGRAM")
                         .help("remove program from list of commands to execute inside; for example, copy-and-paste value shown in `hardshare list` here"))
                    .arg(Arg::with_name("monitor_prog")
                        .long("monitor-prog")
                        .value_name("PROGRAM")
                        .help("declare program to run in a monitor cycle; use `-` to declare none"))
                    .arg(Arg::with_name("hook_emails")
                        .long("hook-emails")
                        .value_name("ADDRESSES")
                        .help("specify email addresses to receive alerts; use `-` to indicate none"))
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
                         .help("mistyproxy: declare IP address of Misty robot"))
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
        .subcommand(SubCommand::with_name("lock")
                    .about("Lock a workspace deployment to prevent new instances")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of target workspace deployment (can be unique prefix); this argument is not required if there is only 1 workspace deployment")))
        .subcommand(SubCommand::with_name("unlock")
                    .about("Unlock a workspace deployment to allow new instances, depending on access rules")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of target workspace deployment (can be unique prefix); this argument is not required if there is only 1 workspace deployment")))
        .subcommand(SubCommand::with_name("stop-ad")
                    .about("Mark as unavailable; optionally wait for current instance to finish")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of workspace deployment to stop advertising (can be unique prefix); this argument is not required if there is only 1 workspace deployment")))
        .subcommand(SubCommand::with_name("register")
                    .about("Register new workspace deployment")
                    .arg(Arg::with_name("permit_more")
                         .long("permit-more")
                         .help("Permits registration of more than 1 wdeployment; default is to fail if local configuration already has wdeployment declared")))
        .subcommand(SubCommand::with_name("declare-org")
                    .about("Declare default organization for commands; for example, `register` will mark the owner as this organization or, if none, the user")
                    .arg(Arg::with_name("org_name")
                         .value_name("ORG")))
        .subcommand(SubCommand::with_name("status")
                    .about("Get information about a running hardshare client, if present"))
        .subcommand(SubCommand::with_name("reload")
                    .about("Reload configuration in a running hardshare client"))
        .subcommand(SubCommand::with_name("dissolve")
                    .about("Dissolve this workspace deployment, making it unavailable for any future use (THIS CANNOT BE UNDONE)")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")))
        .subcommand(SubCommand::with_name("attach-camera")
                    .about("Attach camera stream to workspace deployments")
                    .arg(Arg::with_name("camera_path")
                         .value_name("PATH")
                         .help("on Linux, default is /dev/video0"))
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .multiple(true)
                         .help("id of workspace deployments for camera attachment; if --crop is given, then IDs are read from there instead"))
                    .arg(Arg::with_name("attach_camera_res")
                         .long("width-height")
                         .value_name("W,H")
                         .help("width and height of captured images; default depends on the supporting drivers"))
                    .arg(Arg::with_name("attach_camera_crop_config")
                         .long("crop")
                         .value_name("CROPCONFIG")
                         .help("image crop configuration; default: all wdeployments get full images")))
        .subcommand(SubCommand::with_name("check")
                    .about("Check configuration, dependencies, runtime behavior")
                    .arg(Arg::with_name("all")
                         .long("all")
                         .help("check all deployments in local configuration"))
                    .arg(Arg::with_name("fail_fast")
                         .long("fail-fast")
                         .help("quit upon first detected error"))
                    .arg(Arg::with_name("camera")
                         .long("camera")
                         .help("check camera and image capture (not streaming)"))
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of workspace deployment to check; if neither --all nor ID is given, then check whether a deployment with the default configuration has all requirements satisfied")))
        .subcommand(SubCommand::with_name("stop-cameras")
                    .about("Stop camera streams previously started by attach-camera")
                    .arg(Arg::with_name("all_cameras")
                         .short("a")
                         .long("all")
                         .help("Stop all attached cameras associated with this user account, whether or not started on this host"))
                    .arg(Arg::with_name("force")
                         .short("f")
                         .long("force")
                         .help("Mark cameras as stopped on the server, whether or not the local process can be found")))
        .subcommand(SubCommand::with_name("monitor")
                    .about("Detect and handle errors in a deployment")
                    .arg(Arg::with_name("id_prefix")
                         .value_name("ID")
                         .help("id of workspace deployment to monitor"))
                    .arg(Arg::with_name("loop")
                        .long("loop")
                        .value_name("DURATION")
                        .help("Repeat monitor checks every DURATION seconds")))
        ;

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
                PrintingFormat::Json
            } else if given_pformat_lower == "yaml" {
                PrintingFormat::Yaml
            } else {
                return CliError::new(format!("unrecognized format: {given_pformat}").as_str(), 1);
            }
        }
        None => PrintingFormat::Default,
    };

    let bindaddr = format!("127.0.0.1:{}", matches.value_of("daemonport").unwrap());

    if matches.is_present("version") || matches.subcommand_matches("version").is_some() {
        println!(crate_version!());
    } else if matches.subcommand_matches("init").is_some() {
        return init_subcommand();
    } else if let Some(matches) = matches.subcommand_matches("list") {
        return list_subcommand(matches, pformat);
    } else if let Some(matches) = matches.subcommand_matches("config") {
        return config_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("config-addon") {
        return config_addon_subcommand(matches, pformat);
    } else if let Some(matches) = matches.subcommand_matches("rules") {
        return rules_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("ad") {
        return ad_subcommand(matches, &bindaddr);
    } else if let Some(matches) = matches.subcommand_matches("stop-ad") {
        return stop_ad_subcommand(matches, &bindaddr);
    } else if let Some(matches) = matches.subcommand_matches("register") {
        return register_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("declare-org") {
        return declare_default_org_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("lock") {
        return lock_wdeplyoment_subcommand(matches, true);
    } else if let Some(matches) = matches.subcommand_matches("unlock") {
        return lock_wdeplyoment_subcommand(matches, false);
    } else if matches.subcommand_matches("status").is_some() {
        return status_subcommand(&bindaddr, pformat);
    } else if let Some(matches) = matches.subcommand_matches("dissolve") {
        return dissolve_subcommand(matches);
    } else if matches.subcommand_matches("reload").is_some() {
        return reload_subcommand(&bindaddr);
    } else if let Some(matches) = matches.subcommand_matches("check") {
        return check_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("monitor") {
        return monitor_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("attach-camera") {
        return attach_camera_subcommand(matches);
    } else if let Some(matches) = matches.subcommand_matches("stop-cameras") {
        return stop_cameras_subcommand(matches);
    } else {
        return CliError::new("No command given. Try `hardshare -h`", 1);
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
        print_config_w(&mut buf, &lconf, &None, PrintingFormat::Json, true).unwrap();
        let buf_parsing_result: Result<serde_json::Value, serde_json::Error> =
            serde_json::from_slice(&buf);
        assert!(buf_parsing_result.is_ok());
    }
}
