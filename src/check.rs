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

use std::process::Command;

use crate::control;
use crate::mgmt::{self, CProvider, Config, WDeployment};


#[derive(Debug)]
pub struct Error {
    pub description: Option<String>,
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.description {
            Some(d) => write!(f, "{}", d),
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
        Err(err) => return Err(format!("error calling `docker version`: {}", err)),
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
        Err(err) => return Err(format!("error calling `podman version`: {}", err)),
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
    let status = match Command::new("lxc").args(["list", "-c", "n"]).status() {
        Ok(s) => s,
        Err(err) => return Err(format!("error calling `lxc list`: {}", err)),
    };
    if !status.success() {
        return Err(format!(
            "`lxc list` failed with return code: {:?}",
            status.code()
        ));
    }
    Ok(())
}


fn check_cprovider(cp: &CProvider) -> Result<(), Box<dyn std::error::Error>> {
    if cp == &CProvider::Podman {
        check_podman()?;
    } else if cp == &CProvider::Docker || cp == &CProvider::DockerRootless {
        check_docker(cp == &CProvider::DockerRootless)?;
    } else if cp == &CProvider::Lxd {
        check_lxd()?;
    }
    Ok(())
}


pub fn config(
    local_config: &Config,
    id: &str,
    fail_fast: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let wd_index = match mgmt::find_id_prefix(local_config, Some(id)) {
        Ok(wi) => wi,
        Err(_) => {
            return Err(Error::new(format!(
                "given ID not found in local config: {}",
                id
            )))
        }
    };

    let mut at_least_one_error = false;

    info!("checking configuration of {} ...", id);

    if local_config
        .api_tokens
        .contains_key(&local_config.wdeployments[wd_index].owner)
    {
        if local_config.api_tokens[&local_config.wdeployments[wd_index].owner].is_empty() {
            let msg = format!("no valid API tokens for managing {}", id);
            if fail_fast {
                return Err(Error::new(&msg));
            }
            at_least_one_error = true;
            println!("{}", msg);
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
                let msg = format!("no valid API tokens for managing {}", id);
                if fail_fast {
                    return Err(Error::new(&msg));
                }
                at_least_one_error = true;
                println!("{}", msg);
            } else {
                match org_with_match {
                    Some(o) => {
                        println!("warning: valid API token for managing {} with organization {}, which is not default org", id, o);
                    }
                    None => {
                        println!(
                            "warning: valid API token for managing {} not in default org",
                            id
                        );
                    }
                }
            }
        }
    }

    if let Err(err) = check_cprovider(&local_config.wdeployments[wd_index].cprovider) {
        return Err(Error::new(format!(
            "{}\nIs {} installed correctly?",
            err, &local_config.wdeployments[wd_index].cprovider
        )));
    }

    info!("simulating instance launch ...");
    let cname = "check";
    if let Err(err) = control::CurrentInstance::launch_container(
        &local_config.wdeployments[wd_index],
        cname,
        "checkkey",
    ) {
        let mut msg = format!("caught while creating test container: {}", err);
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
        println!("{}", msg);
    }

    if let Err(err) =
        control::CurrentInstance::destroy_container(&local_config.wdeployments[wd_index], cname)
    {
        let msg = format!("caught while destroying test container: {}", err);
        if fail_fast {
            return Err(Error::new(&msg));
        }
        at_least_one_error = true;
        println!("{}", msg);
    }

    if at_least_one_error {
        Err(Error::new_empty())
    } else {
        Ok(())
    }
}


pub fn all_configurations(
    local_config: &Config,
    fail_fast: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut at_least_one_error = false;
    for wd in local_config.wdeployments.iter() {
        if let Err(err) = config(local_config, &wd.id, fail_fast) {
            let msg = format!("{}: {}", &wd.id, err);
            if fail_fast {
                return Err(Error::new(&msg));
            }
            at_least_one_error = true;
            println!("{}", msg);
        }
    }
    if at_least_one_error {
        Err(Error::new_empty())
    } else {
        Ok(())
    }
}


pub fn defaults(fail_fast: bool) -> Result<(), Box<dyn std::error::Error>> {
    let mut at_least_one_error = false;

    let wdeployment = WDeployment::new_min("68a1be97-9365-4007-b726-14c56bd69eef", "owner");

    if let Err(err) = check_cprovider(&wdeployment.cprovider) {
        return Err(Error::new(format!(
            "{}\nIs {} installed correctly?",
            err, &wdeployment.cprovider
        )));
    }

    info!("simulating instance launch ...");
    let cname = "check";
    if let Err(err) = control::CurrentInstance::launch_container(&wdeployment, cname, "checkkey") {
        let msg = format!("caught while creating test container: {}", err);
        if fail_fast {
            return Err(Error::new(&msg));
        }
        at_least_one_error = true;
        println!("{}", msg);
    }

    if let Err(err) = control::CurrentInstance::destroy_container(&wdeployment, cname) {
        let msg = format!("caught while destroying test container: {}", err);
        if fail_fast {
            return Err(Error::new(&msg));
        }
        at_least_one_error = true;
        println!("{}", msg);
    }

    if at_least_one_error {
        Err(Error::new_empty())
    } else {
        Ok(())
    }
}
