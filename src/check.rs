// Copyright (C) 2023 rerobots, Inc.
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

use std::process::{Command, Stdio};

use crate::mgmt::{self, CProvider, Config, WDeployment};
use crate::{api, camera, control, monitor};

#[derive(Debug)]
pub struct Error {
    pub description: Option<String>,
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.description {
            Some(d) => write!(f, "{d}"),
            None => write!(f, ""),
        }
    }
}

impl Error {
    pub fn new<S>(description: S) -> Box<Self>
    where
        S: ToString,
    {
        Box::new(Error {
            description: Some(description.to_string()),
        })
    }
    fn new_empty() -> Box<Self> {
        Box::new(Error { description: None })
    }
}

fn check_docker(rootless: bool) -> Result<(), String> {
    info!(
        "checking availability of docker{}",
        if rootless { " (rootless)" } else { "" }
    );
    let output = match Command::new("docker").arg("version").output() {
        Ok(x) => x,
        Err(err) => return Err(format!("error calling `docker version`: {err}")),
    };
    if !output.status.success() {
        return Err(format!(
            "`docker version` failed with return code: {:?}",
            output.status.code()
        ));
    }
    Ok(())
}

fn check_podman() -> Result<(), String> {
    info!("checking availability of podman");
    let output = match Command::new("podman").arg("version").output() {
        Ok(x) => x,
        Err(err) => return Err(format!("error calling `podman version`: {err}")),
    };
    if !output.status.success() {
        return Err(format!(
            "`podman version` failed with return code: {:?}",
            output.status.code()
        ));
    }
    Ok(())
}

fn check_lxd() -> Result<(), String> {
    info!("checking availability of lxd");
    // `lxc list` because it has nonzero exit code if the server is unreachable
    // whereas `lxc version` indicates success, only shows the fault in stdout
    let status = match Command::new("lxc").args(["list", "-c", "n"]).status() {
        Ok(s) => s,
        Err(err) => return Err(format!("error calling `lxc list`: {err}")),
    };
    if !status.success() {
        return Err(format!(
            "`lxc list` failed with return code: {:?}",
            status.code()
        ));
    }
    Ok(())
}

pub fn check_proxy(wd: &WDeployment) -> Result<(), String> {
    if wd.cargs.is_empty() {
        return Err(
            "Proxy is not configured. Try `hardshare config --assign-proxy-command`".into(),
        );
    }
    let mut child = match Command::new(&wd.cargs[0])
        .args(wd.cargs[1..].iter())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(err) => return Err(err.to_string()),
    };
    match child.kill() {
        Ok(()) => (),
        Err(err) => return Err(err.to_string()),
    }
    Ok(())
}

fn check_cprovider(wd: &WDeployment) -> Result<(), String> {
    match wd.cprovider {
        CProvider::Podman => check_podman(),
        CProvider::Docker | CProvider::DockerRootless => {
            check_docker(wd.cprovider == CProvider::DockerRootless)
        }
        CProvider::Lxd => check_lxd(),
        CProvider::Proxy => check_proxy(wd),
    }
}

fn check_deployment_in_remote(
    id: &str,
    remote_config: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut found = false;
    for wd in remote_config["wdeployments"].as_array().unwrap().iter() {
        if wd["id"] == id {
            found = true;
            break;
        }
    }
    if !found {
        return Err(Error::new("This deployment is not known by rerobots! Compare with your devices shown at https://rerobots.net/hardshare\nDid you manually edit local configuration files?"));
    }
    Ok(())
}

pub fn config(
    local_config: &Config,
    check_camera: bool,
    id: &str,
    remote_config: Option<&serde_json::Value>,
    fail_fast: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let wd_index = match mgmt::find_id_prefix(local_config, Some(id)) {
        Ok(wi) => wi,
        Err(_) => {
            return Err(Error::new(format!(
                "given ID not found in local config: {id}"
            )))
        }
    };

    let mut at_least_one_error = false;

    info!("checking configuration of {id} ...");

    match remote_config {
        Some(rc) => {
            let res = check_deployment_in_remote(id, rc);
            if let Err(err) = res {
                at_least_one_error = true;
                if fail_fast {
                    return Err(err);
                }
                println!("{id}: {err}");
            }
        }
        None => {
            let ac = api::HSAPIClient::new();
            match ac.get_remote_config(false) {
                Ok(rc) => {
                    let res = check_deployment_in_remote(id, &rc);
                    if let Err(err) = res {
                        at_least_one_error = true;
                        if fail_fast {
                            return Err(err);
                        }
                        println!("{id}: {err}");
                    }
                }
                Err(err) => {
                    let msg = format!("caught while checking registration on server: {err}");
                    if fail_fast {
                        return Err(Error::new(&msg));
                    }
                    at_least_one_error = true;
                    println!("{msg}");
                }
            };
        }
    }

    if check_camera {
        if let Err(err) = camera::check_camera(&camera::get_default_dev()) {
            let msg = format!("caught while checking camera: {err}");
            if fail_fast {
                return Err(Error::new(&msg));
            }
            at_least_one_error = true;
            println!("{msg}");
        }
    }

    if local_config
        .api_tokens
        .contains_key(&local_config.wdeployments[wd_index].owner)
    {
        if local_config.api_tokens[&local_config.wdeployments[wd_index].owner].is_empty() {
            let msg = format!("no valid API tokens for managing {id}");
            if fail_fast {
                return Err(Error::new(&msg));
            }
            at_least_one_error = true;
            println!("{msg}");
        }
    } else {
        let default_org = match &local_config.default_org {
            Some(default_org) => default_org.as_str(),
            None => "()",
        };
        let mut at_least_one_in_default = false;
        if local_config.api_tokens.contains_key(default_org) {
            for token_path in local_config.api_tokens[default_org].iter() {
                let username = mgmt::get_username(token_path)?;
                if username == local_config.wdeployments[wd_index].owner {
                    at_least_one_in_default = true;
                    break;
                }
            }
        }
        if !at_least_one_in_default {
            let mut at_least_one = false;
            let mut org_with_match: Option<String> = None;
            for org_name in local_config.api_tokens.keys() {
                if org_name == default_org {
                    continue;
                }
                for token_path in local_config.api_tokens[org_name].iter() {
                    let username = mgmt::get_username(token_path)?;
                    if username == local_config.wdeployments[wd_index].owner {
                        at_least_one = true;
                        org_with_match = Some(org_name.to_string());
                        break;
                    }
                }
                if at_least_one {
                    break;
                }
            }
            if !at_least_one {
                let msg = format!("no valid API tokens for managing {id}");
                if fail_fast {
                    return Err(Error::new(&msg));
                }
                at_least_one_error = true;
                println!("{msg}");
            } else {
                match org_with_match {
                    Some(o) => {
                        println!("warning: valid API token for managing {id} with organization {o}, which is not default org");
                    }
                    None => {
                        println!("warning: valid API token for managing {id} not in default org");
                    }
                }
            }
        }
    }

    if let Err(err) = check_cprovider(&local_config.wdeployments[wd_index]) {
        return Err(Error::new(format!(
            "{}\nIs {} installed correctly?",
            err, &local_config.wdeployments[wd_index].cprovider
        )));
    }

    monitor::run_dry(local_config, wd_index)?;

    info!("simulating instance launch ...");
    let cname = "check";
    if let Err(err) = control::CurrentInstance::launch_container(
        &local_config.wdeployments[wd_index],
        cname,
        "checkkey",
    ) {
        let mut msg = format!("caught while creating test container: {err}");
        if fail_fast {
            if local_config.wdeployments[wd_index].cprovider != CProvider::Proxy {
                msg += &format!(
                    "\nYou may need to manually stop or remove it using {}: {}",
                    local_config.wdeployments[wd_index].cprovider, cname
                );
            }
            return Err(Error::new(&msg));
        }
        at_least_one_error = true;
        println!("{msg}");
    }

    info!("simulating instance terminate ...");
    if let Err(err) =
        control::CurrentInstance::destroy_container(&local_config.wdeployments[wd_index], cname)
    {
        let msg = format!("caught while destroying test container: {err}");
        if fail_fast {
            return Err(Error::new(&msg));
        }
        at_least_one_error = true;
        println!("{msg}");
    }

    if at_least_one_error {
        Err(Error::new_empty())
    } else {
        Ok(())
    }
}

pub fn all_configurations(
    local_config: &Config,
    check_camera: bool,
    fail_fast: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut at_least_one_error = false;

    let ac = api::HSAPIClient::new();
    let remote_config = match ac.get_remote_config(false) {
        Ok(rc) => Some(rc),
        Err(err) => {
            let msg = format!("caught while checking registration on server: {err}");
            if fail_fast {
                return Err(Error::new(&msg));
            }
            at_least_one_error = true;
            println!("{msg}");
            None
        }
    };

    if check_camera {
        if let Err(err) = camera::check_camera(&camera::get_default_dev()) {
            let msg = format!("caught while checking camera: {err}");
            if fail_fast {
                return Err(Error::new(&msg));
            }
            at_least_one_error = true;
            println!("{msg}");
        }
    }

    for wd in local_config.wdeployments.iter() {
        if let Err(err) = config(
            local_config,
            false,
            &wd.id,
            remote_config.as_ref(),
            fail_fast,
        ) {
            let msg = format!("{}: {}", &wd.id, err);
            if fail_fast {
                return Err(Error::new(&msg));
            }
            at_least_one_error = true;
            println!("{msg}");
        }
    }
    if at_least_one_error {
        Err(Error::new_empty())
    } else {
        Ok(())
    }
}

pub fn defaults(check_camera: bool, fail_fast: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut at_least_one_error = false;

    let wdeployment = WDeployment::new_min("68a1be97-9365-4007-b726-14c56bd69eef", "owner");

    if check_camera {
        if let Err(err) = camera::check_camera(&camera::get_default_dev()) {
            let msg = format!("caught while checking camera: {err}");
            if fail_fast {
                return Err(Error::new(&msg));
            }
            at_least_one_error = true;
            println!("{msg}");
        }
    }

    if let Err(err) = check_cprovider(&wdeployment) {
        return Err(Error::new(format!(
            "{}\nIs {} installed correctly?",
            err, &wdeployment.cprovider
        )));
    }

    info!("simulating instance launch ...");
    let cname = "check";
    if let Err(err) = control::CurrentInstance::launch_container(&wdeployment, cname, "checkkey") {
        let msg = format!("caught while creating test container: {err}");
        if fail_fast {
            return Err(Error::new(&msg));
        }
        at_least_one_error = true;
        println!("{msg}");
    }

    info!("simulating instance terminate ...");
    if let Err(err) = control::CurrentInstance::destroy_container(&wdeployment, cname) {
        let msg = format!("caught while destroying test container: {err}");
        if fail_fast {
            return Err(Error::new(&msg));
        }
        at_least_one_error = true;
        println!("{msg}");
    }

    if at_least_one_error {
        Err(Error::new_empty())
    } else {
        Ok(())
    }
}
