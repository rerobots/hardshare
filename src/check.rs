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

use crate::mgmt::{self, Config};


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


pub fn config(local_config: &Config, id: &str, fail_fast: bool) -> Result<(), String> {
    let wd_index = match mgmt::find_id_prefix(local_config, Some(id)) {
        Ok(wi) => wi,
        Err(_) => return Err(format!("given ID not found in local config: {}", id)),
    };

    info!("checking configuration of {} ...", id);

    if local_config
        .api_tokens
        .contains_key(&local_config.wdeployments[wd_index].owner)
    {
        if local_config.api_tokens[&local_config.wdeployments[wd_index].owner].is_empty() {
            return Err(format!("no valid API tokens for managing {}", id));
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
                return Err(format!("no valid API tokens for managing {}", id));
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

    if local_config.wdeployments[wd_index].cprovider == "podman" {
        check_podman()?;
    } else if local_config.wdeployments[wd_index].cprovider == "docker"
        || local_config.wdeployments[wd_index].cprovider == "docker-rootless"
    {
        check_docker(local_config.wdeployments[wd_index].cprovider == "docker-rootless")?;
    } else if local_config.wdeployments[wd_index].cprovider == "lxd" {
        check_lxd()?;
    }

    Ok(())
}


pub fn all_configurations(local_config: &Config, fail_fast: bool) -> Result<(), String> {
    for wd in local_config.wdeployments.iter() {
        if let Err(err) = config(local_config, &wd.id, fail_fast) {
            return Err(format!("{}: {}", &wd.id, err));
        }
    }
    Ok(())
}


pub fn defaults(fail_fast: bool) -> Result<(), String> {
    check_docker(false)?;
    Ok(())
}
