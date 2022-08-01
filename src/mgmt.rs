// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::collections::{BTreeMap, HashMap};
use std::process::{Command, Stdio};

extern crate serde;
extern crate serde_json;
use serde::{Deserialize, Serialize};

extern crate home;

extern crate openssl;
use openssl::hash::MessageDigest;
use openssl::pkey::PKey;

extern crate jwt;
use jwt::algorithm::openssl::PKeyWithDigest;
use jwt::VerifyWithKey;
use jwt::{Claims, Header, Token};


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

fn error<T>(msg: &str) -> Result<T, Box<dyn std::error::Error>> {
    Err(Box::new(MgmtError {
        msg: String::from(msg),
    }))
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    version: u16,
    pub wdeployments: Vec<HashMap<String, serde_json::Value>>,
    pub ssh_key: String,

    // organization name | () -> [path0, path1, ...]
    // where "()" indicates no org
    #[serde(default)]
    pub api_tokens: HashMap<String, Vec<String>>,

    #[serde(default)]
    pub err_api_tokens: Option<HashMap<String, String>>,

    #[serde(default)]
    pub default_org: Option<String>,

    #[serde(default)]
    pub known_orgs: Vec<String>,
}

impl Config {
    pub fn new() -> Config {
        Config {
            version: 0,
            wdeployments: vec![],
            ssh_key: "".to_string(),
            api_tokens: HashMap::new(),
            err_api_tokens: None,
            default_org: None,
            known_orgs: vec![],
        }
    }
}


fn get_base_path() -> Option<std::path::PathBuf> {
    let home_dir = match home::home_dir() {
        Some(s) => s,
        None => return None,
    };
    Some(home_dir.join(".rerobots"))
}


type APITokensInfo = (HashMap<String, Vec<String>>, HashMap<String, String>);

pub fn list_local_api_tokens(
    collect_errors: bool,
) -> Result<APITokensInfo, Box<dyn std::error::Error>> {
    let base_path = get_base_path().unwrap();
    list_local_api_tokens_bp(&base_path, collect_errors)
}

fn list_local_api_tokens_bp(
    base_path: &std::path::Path,
    collect_errors: bool,
) -> Result<APITokensInfo, Box<dyn std::error::Error>> {
    let mut likely_tokens = HashMap::new();
    let mut errored_tokens = HashMap::new();
    if !base_path.exists() {
        return Ok((likely_tokens, errored_tokens));
    }
    let path = base_path.join("tokens");
    if !path.exists() {
        return Ok((likely_tokens, errored_tokens));
    }

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            warn!("skipping subdirectory in tokens directory");
            continue;
        }
        let path = entry.path();
        let rawtok = String::from(
            String::from_utf8(std::fs::read(&path).unwrap())
                .unwrap()
                .trim(),
        );
        match get_jwt_claims(&rawtok) {
            Ok(claims) => {
                let path = String::from(path.to_str().unwrap());
                let org;
                if claims.contains_key("org") {
                    org = claims["org"].as_str().unwrap();
                } else {
                    org = "()";
                }
                if likely_tokens.contains_key(org) {
                    let org_tokens = likely_tokens.get_mut(org).unwrap();
                    org_tokens.push(path);
                } else {
                    likely_tokens.insert(org.into(), vec![path]);
                }
            }
            Err(err) => {
                if collect_errors {
                    errored_tokens.insert(String::from(path.to_str().unwrap()), err);
                }
            }
        }
    }

    Ok((likely_tokens, errored_tokens))
}


pub fn get_local_config(
    create_if_empty: bool,
    collect_errors: bool,
) -> Result<Config, Box<dyn std::error::Error>> {
    let base_path = get_base_path().unwrap();
    get_local_config_bp(&base_path, create_if_empty, collect_errors)
}

pub fn get_local_config_bp(
    base_path: &std::path::Path,
    create_if_empty: bool,
    collect_errors: bool,
) -> Result<Config, Box<dyn std::error::Error>> {
    if !base_path.exists() {
        if create_if_empty {
            std::fs::create_dir(&base_path)?;
            std::fs::create_dir(base_path.join("tokens"))?;
            std::fs::create_dir(base_path.join("ssh"))?;
        } else {
            return error("no configuration data found");
        }
    }
    let path = base_path.join("main");
    if !path.exists() {
        if create_if_empty {
            let mut init = Config::new();
            let sshpath = base_path.join("ssh").join("tun");
            let exitcode = Command::new("ssh-keygen")
                .arg("-N")
                .arg("")
                .arg("-f")
                .arg(&sshpath)
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
    let res = list_local_api_tokens(collect_errors)?;
    config.api_tokens = res.0;
    if collect_errors {
        config.err_api_tokens = Some(res.1);
    }
    Ok(config)
}


pub fn append_urls(config: &mut Config) {
    let prefix = "https://rerobots.net/workspace/";
    for wd in config.wdeployments.iter_mut() {
        if !wd.contains_key("url") {
            wd.insert(
                "url".to_string(),
                serde_json::Value::String(format!("{}{}", prefix, wd["id"].as_str().unwrap())),
            );
        }
    }
}


pub fn add_token_file(path: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let rawtok = String::from(
        String::from_utf8(std::fs::read(&path)?)?
            .trim(),
    );
    let org;
    match get_jwt_claims(&rawtok) {
        Ok(claims) => {
            if claims.contains_key("org") {
                org = Some(String::from(claims["org"].as_str().unwrap()))
            } else {
                org = None
            }
        }
        Err(err) => return error(err.as_str()),
    };

    let base_path = get_base_path().unwrap();
    let tokens_dir = base_path.join("tokens");
    if !tokens_dir.exists() {
        std::fs::create_dir(&tokens_dir)?
    }
    let from_filename = std::path::Path::new(path).file_name().unwrap();
    let mut target_path = tokens_dir.join(from_filename);
    if target_path.exists() {
        let utime = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();
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
    if std::fs::rename(path, &target_path).is_err() {
        std::fs::copy(path, &target_path)?;
        std::fs::remove_file(path)?;
    }
    Ok(org)
}


pub fn find_id_prefix(
    config: &Config,
    id_prefix: Option<&str>,
) -> Result<usize, Box<dyn std::error::Error>> {
    if let Some(id_prefix) = id_prefix {
        let mut candidates = vec![];

        for (j, wd) in config.wdeployments.iter().enumerate() {
            let this_wdid = wd["id"].as_str().unwrap();
            if this_wdid.starts_with(id_prefix) {
                candidates.push((j, this_wdid));
            }
        }
        if candidates.len() > 1 {
            let candidates: Vec<&str> = candidates.iter().map(|&val| val.1).collect();
            error(
                format!(
                    "given prefix matches more than 1 workspace deployment: {}",
                    candidates.join(", ")
                )
                .as_str(),
            )
        } else if candidates.is_empty() {
            error("given prefix does not match any workspace deployments")
        } else {
            Ok(candidates[0].0)
        }
    } else if config.wdeployments.len() == 1 {
        Ok(0)
    } else if config.wdeployments.is_empty() {
        error("no workspace deployment in local configuration.")
    } else {
        error("ambiguous command: more than 1 workspace deployment defined.")
    }
}


pub fn modify_local(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let base_path = get_base_path().unwrap();
    if !base_path.exists() {
        return error("no configuration data found");
    }
    let path = base_path.join("main");
    if !path.exists() {
        return error("no configuration data found");
    }
    std::fs::write(&path, serde_json::to_string(&config)?)?;
    Ok(())
}


fn get_jwt_claims(rawtok: &str) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let alg = PKeyWithDigest {
        digest: MessageDigest::sha256(),
        key: PKey::public_key_from_pem(WEBUI_PUBLIC_KEY).unwrap(),
    };
    let now = std::time::SystemTime::now();
    let utime = now.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let result: Result<BTreeMap<String, serde_json::Value>, jwt::error::Error> =
        rawtok.verify_with_key(&alg);
    match result {
        Ok(claims) => {
            let exp = claims["exp"].as_u64().unwrap();
            if exp < utime {
                Err("expired".into())
            } else {
                Ok(claims)
            }
        }
        Err(err) => match err {
            jwt::error::Error::InvalidSignature => Err("invalid signature".into()),
            _ => Err("unknown error".into()),
        },
    }
}


#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use tempfile::tempdir;

    use super::find_id_prefix;
    use super::get_local_config_bp;
    use super::list_local_api_tokens_bp;
    use super::Config;


    #[test]
    fn configuration_directory_suffix() {
        let base_path = super::get_base_path().unwrap();
        assert!(base_path.ends_with(".rerobots"));
    }


    #[test]
    fn find_id() {
        let local_config = Config::new();
        assert!(find_id_prefix(&local_config, Some("a")).is_err());

        let local_config: Config = serde_json::from_str(
            r#"
            {
                "version": 0,
                "wdeployments": [
                    {
                        "id": "2d6039bc-7c83-4d46-8567-c8df4711c386",
                        "owner": "scott",
                        "cprovider": "proxy",
                        "cargs": [],
                        "image": null,
                        "terminate": [],
                        "init_inside": [],
                        "container_name": "rrc"
                    },
                    {
                        "id": "68a1be97-9365-4007-b726-14c56bd69eef",
                        "owner": "bilbo",
                        "cprovider": "podman",
                        "cargs": [],
                        "image": "rerobots/hs-generic",
                        "terminate": [],
                        "init_inside": [],
                        "container_name": "rrc"
                    }
                ],
                "ssh_key": "/home/scott/.rerobots/ssh/tun"
            }"#,
        )
        .unwrap();
        assert!(find_id_prefix(&local_config, Some("a")).is_err());
        let wd_index = find_id_prefix(&local_config, Some("2")).unwrap();
        assert_eq!(wd_index, 0);
        let wd_index = find_id_prefix(&local_config, Some("6")).unwrap();
        assert_eq!(wd_index, 1);
    }


    #[test]
    fn no_config() {
        let td = tempdir().unwrap();
        let base_path = td.path().join(".rerobots");
        assert!(get_local_config_bp(&base_path, false, false).is_err());
    }


    #[test]
    fn init_config() {
        let td = tempdir().unwrap();
        let base_path = td.path().join(".rerobots");
        let lconf = get_local_config_bp(&base_path, true, false).unwrap();
        assert_eq!(lconf.wdeployments.len(), 0);
        assert_ne!(lconf.ssh_key.len(), 0);
    }


    #[test]
    fn no_saved_api_tokens() {
        let td = tempdir().unwrap();
        let base_path = td.path().join(".rerobots");
        let (likely_tokens, errored_tokens) = list_local_api_tokens_bp(&base_path, false).unwrap();
        assert_eq!(likely_tokens.len(), 0);
        assert_eq!(errored_tokens.len(), 0);
    }
}
