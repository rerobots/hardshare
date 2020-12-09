// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::collections::HashMap;
use std::process::{Command, Stdio};

extern crate serde_json;
extern crate serde;
use serde::{Serialize, Deserialize};

extern crate home;


struct MgmtError {
    msg: String,
}
impl std::error::Error for MgmtError {}

impl std::fmt::Display for MgmtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::fmt::Debug for MgmtError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

fn error(msg: &str) -> Result<Config, Box<dyn std::error::Error>> {
    Err(Box::new(MgmtError { msg: String::from(msg) }))
}


#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    version: u16,
    wdeployments: Vec<HashMap<String, serde_json::Value>>,
    ssh_key: String,

    #[serde(default)]
    keys: Vec<String>,
}


fn get_base_path() -> Option<std::path::PathBuf> {
    let home_dir = match home::home_dir() {
        Some(s) => s,
        None => return None
    };
    Some(home_dir.join(".rerobots"))
}


pub fn list_local_keys(collect_errors: bool) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    Ok(vec![])
}


pub fn get_local_config(create_if_empty: bool, collect_errors: bool) -> Result<Config, Box<dyn std::error::Error>> {
    let base_path = get_base_path().unwrap();
    if !base_path.exists() {
        if create_if_empty {
            std::fs::create_dir(&base_path)?;
            std::fs::create_dir(base_path.join("keys"))?;
            std::fs::create_dir(base_path.join("ssh"))?;
        } else {
            return error("no configuration data found");
        }
    }
    let path = base_path.join("main");
    if !path.exists() {
        if create_if_empty {
            let mut init = Config {
                version: 0,
                wdeployments: vec![],
                ssh_key: "".to_string(),
                keys: vec![],
            };
            let sshpath = base_path.join("ssh").join("tun");
            let exitcode = Command::new("ssh-keygen")
                .arg("-N").arg("")
                .arg("-f").arg(&sshpath)
                .stdout(Stdio::piped())
                .spawn()
                .expect("failed to call ssh-keygen")
                .wait()
                .expect("failed to wait on ssh-keygen");
            if !exitcode.success() {
                return error("failed to create SSH keys");
            }
            init.ssh_key = String::from(sshpath.to_str().unwrap());
            std::fs::write(&path, serde_json::to_string(&init)?)?;
        } else {
            return error("no configuration data found");
        }
    }
    let config_raw = std::fs::read_to_string(path)?;
    let mut config = serde_json::from_str(config_raw.as_str())?;
    Ok(config)
}
