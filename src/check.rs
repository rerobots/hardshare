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
    let output = match Command::new("docker").arg("version").output() {
        Ok(x) => x,
        Err(err) => return Err(format!("error calling `docker version`: {}", err)),
    };
    Ok(())
}


fn check_podman() -> Result<(), String> {
    let output = match Command::new("podman").arg("version").output() {
        Ok(x) => x,
        Err(err) => return Err(format!("error calling `podman version`: {}", err)),
    };
    Ok(())
}


fn check_lxd() -> Result<(), String> {
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


pub fn config(local_config: &Config, id: &str) -> Result<(), String> {
    let wd_index = match mgmt::find_id_prefix(local_config, Some(id)) {
        Ok(wi) => wi,
        Err(_) => return Err(format!("given ID not found in local config: {}", id)),
    };

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


pub fn all_configurations(local_config: &Config) -> Result<(), String> {
    Ok(())
}


pub fn defaults() -> Result<(), String> {
    check_docker(false)?;
    Ok(())
}
