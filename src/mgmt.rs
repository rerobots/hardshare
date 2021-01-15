// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::collections::HashMap;
use std::process::{Command, Stdio};

extern crate serde_json;
extern crate serde;
use serde::{Serialize, Deserialize};

extern crate home;

extern crate openssl;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;

extern crate jwt;
use jwt::{Token, Header, Claims};
use jwt::VerifyWithKey;
use jwt::algorithm::openssl::PKeyWithDigest;


// TODO: this should eventually be placed in a public key store
const WEBUI_PUBLIC_KEY: &[u8] = include_bytes!("../keys/webui-public.pem");


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
    pub wdeployments: Vec<HashMap<String, serde_json::Value>>,
    pub ssh_key: String,

    #[serde(default)]
    pub keys: Vec<String>,

    #[serde(default)]
    pub err_keys: Option<HashMap<String, String>>,
}


fn get_base_path() -> Option<std::path::PathBuf> {
    let home_dir = match home::home_dir() {
        Some(s) => s,
        None => return None
    };
    Some(home_dir.join(".rerobots"))
}


pub fn list_local_keys(collect_errors: bool) -> Result<(Vec<String>, HashMap<String, String>), Box<dyn std::error::Error>> {
    let base_path = get_base_path().unwrap();
    let mut likely_keys = Vec::new();
    let mut errored_keys = HashMap::new();
    if !base_path.exists() {
        return Ok((likely_keys, errored_keys));
    }
    let path = base_path.join("keys");
    if !path.exists() {
        return Ok((likely_keys, errored_keys));
    }

    let alg = PKeyWithDigest {
        digest: MessageDigest::sha256(),
        key: PKey::public_key_from_pem(WEBUI_PUBLIC_KEY).unwrap(),
    };
    let now = std::time::SystemTime::now();
    let utime = now.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

    for entry in std::fs::read_dir(path)? {
        let path = entry?.path();
        let rawtok = String::from(String::from_utf8(std::fs::read(&path).unwrap()).unwrap().trim());
        let result: Result<Token<Header, Claims, _>, jwt::error::Error> = rawtok.verify_with_key(&alg);
        match result {
            Ok(tok) => {
                let claims = tok.claims();
                if claims.registered.expiration.unwrap() < utime {
                    if collect_errors {
                        errored_keys.insert(String::from(path.to_str().unwrap()), "expired".into());
                    }
                } else {
                    likely_keys.push(String::from(path.to_str().unwrap()));
                }
            },
            Err(err) => match err {
                jwt::error::Error::InvalidSignature => {
                    if collect_errors {
                        errored_keys.insert(String::from(path.to_str().unwrap()), "invalid signature".into());
                    }
                }
                _ => {
                    if collect_errors {
                        errored_keys.insert(String::from(path.to_str().unwrap()), "unknown error".into());
                    }
                }
            }
        };
    }

    Ok((likely_keys, errored_keys))
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
                err_keys: None,
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
    let mut config: Config = serde_json::from_str(config_raw.as_str())?;
    let res = list_local_keys(collect_errors)?;
    config.keys = res.0;
    if collect_errors {
        config.err_keys = Some(res.1);
    }
    Ok(config)
}


pub fn append_urls(config: &mut Config) {
    let prefix = "https://rerobots.net/workspace/";
    for wd in config.wdeployments.iter_mut() {
        if !wd.contains_key("url") {
            wd.insert("url".to_string(),
                      serde_json::Value::String(format!("{}{}", prefix, wd["id"].as_str().unwrap())));
        }
    }
}


pub fn add_token_file(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let base_path = get_base_path().unwrap();
    let tokens_dir = base_path.join("keys");
    if !tokens_dir.exists() {
        std::fs::create_dir(&tokens_dir)?
    }
    let from_filename = std::path::Path::new(path).file_name().unwrap();
    let mut target_path = tokens_dir.join(from_filename);
    if target_path.exists() {
        let utime = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
        let candidate = format!("{}-{}", target_path.to_str().unwrap(), utime);
        target_path = std::path::PathBuf::from(candidate);
    }
    if target_path.exists() {
        for counter in 0.. {
            let candidate = format!("{}-{}", target_path.to_str().unwrap(), counter);
            let candidate = std::path::PathBuf::from(candidate);
            if !candidate.exists() {
                target_path = candidate;
                break;
            }
        }
    }
    std::fs::rename(path, target_path)?;
    Ok(())
}
