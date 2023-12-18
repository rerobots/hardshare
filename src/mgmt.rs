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

use std::collections::{BTreeMap, HashMap};
use std::convert::TryFrom;
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


// TODO: this should eventually be placed in a public key store
#[cfg(not(test))]
const PUBLIC_KEY: &[u8] = include_bytes!("../keys/public.pem");

#[cfg(test)]
const PUBLIC_KEY: &[u8] = include_bytes!("../tests/keys/public.pem");


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


#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum CProvider {
    Docker,
    DockerRootless,
    Lxd,
    Podman,
    Proxy,
}

impl CProvider {
    pub fn get_execname(&self) -> Option<String> {
        if self == &Self::DockerRootless {
            Some("docker".into())
        } else if self == &Self::Proxy {
            None
        } else {
            Some(self.to_string())
        }
    }
}

impl TryFrom<&str> for CProvider {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "docker" => Ok(Self::Docker),
            "docker-rootless" => Ok(Self::DockerRootless),
            "lxd" => Ok(Self::Lxd),
            "podman" => Ok(Self::Podman),
            "proxy" => Ok(Self::Proxy),
            _ => Err("unrecognized cprovider"),
        }
    }
}

impl std::fmt::Display for CProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Docker => write!(f, "docker"),
            Self::DockerRootless => write!(f, "docker-rootless"),
            Self::Lxd => write!(f, "lxd"),
            Self::Podman => write!(f, "podman"),
            Self::Proxy => write!(f, "proxy"),
        }
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WDeployment {
    pub id: String,
    pub owner: String,
    pub cprovider: CProvider,
    pub cargs: Vec<String>,
    pub container_name: String,
    pub init_inside: Vec<String>,
    pub terminate: Vec<String>,

    #[serde(default)]
    pub image: Option<String>,

    #[serde(default)]
    pub url: Option<String>,

    #[serde(default)]
    pub ssh_key: Option<String>,
}

impl WDeployment {
    pub fn from_json(h: &HashMap<String, serde_json::Value>) -> Self {
        let cprovider: CProvider = if h.contains_key("cprovider") {
            CProvider::try_from(h["cprovider"].as_str().unwrap()).unwrap()
        } else {
            CProvider::Docker
        };

        let cargs: Vec<String> = if h.contains_key("cargs") {
            h["cargs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap().to_string())
                .collect()
        } else {
            vec![]
        };

        let container_name = if h.contains_key("container_name") {
            h["container_name"].as_str().unwrap()
        } else {
            "rrc"
        }
        .into();

        let init_inside: Vec<String> = if h.contains_key("init_inside") {
            h["init_inside"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap().to_string())
                .collect()
        } else {
            vec![]
        };

        let terminate: Vec<String> = if h.contains_key("terminate") {
            h["terminate"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap().to_string())
                .collect()
        } else {
            vec![]
        };

        let image = if h.contains_key("image") {
            Some(h["image"].as_str().unwrap().into())
        } else if cprovider != CProvider::Proxy {
            Some("rerobots/hs-generic".into())
        } else {
            None
        };

        let url: Option<String> = if h.contains_key("url") {
            Some(h["url"].as_str().unwrap().into())
        } else {
            None
        };

        WDeployment {
            id: h["id"].as_str().unwrap().into(),
            owner: h["owner"].as_str().unwrap().into(),
            cprovider,
            cargs,
            container_name,
            image,
            init_inside,
            terminate,
            url,

            ssh_key: None,
        }
    }
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    version: u16,
    pub wdeployments: Vec<WDeployment>,
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


pub fn get_base_path() -> Option<std::path::PathBuf> {
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
                let org = if claims.contains_key("org") {
                    claims["org"].as_str().unwrap()
                } else {
                    "()"
                };
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
            std::fs::create_dir(base_path)?;
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
        if wd.url.is_none() {
            wd.url = Some(format!("{}{}", prefix, wd.id));
        }
    }
}


pub fn add_token_file(path: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let rawtok = String::from(String::from_utf8(std::fs::read(path)?)?.trim());
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


pub fn add_ssh_path(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let target = std::path::Path::new(path).canonicalize()?;
    if !target.exists() {
        return error("file does not exist");
    }
    let target_public = target.with_extension("pub");
    if target_public == target {
        return error("public key file cannot be same as secret key file");
    }
    if !target_public.exists() {
        return error("public key file does not exist");
    }
    let mut local_config = match get_local_config(false, false) {
        Ok(lc) => lc,
        Err(err) => return Err(err),
    };
    local_config.ssh_key = match target.to_str() {
        Some(s) => s.into(),
        None => return error("path not given in UTF-8"),
    };
    modify_local(&local_config)
}


pub fn find_id_prefix(
    config: &Config,
    id_prefix: Option<&str>,
) -> Result<usize, Box<dyn std::error::Error>> {
    if let Some(id_prefix) = id_prefix {
        let mut candidates = vec![];

        for (j, wd) in config.wdeployments.iter().enumerate() {
            if wd.id.starts_with(id_prefix) {
                candidates.push((j, wd.id.clone()));
            }
        }
        if candidates.len() > 1 {
            let candidates: Vec<String> = candidates.iter().map(|val| val.1.clone()).collect();
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


pub fn expand_id_prefixes(
    config: &Config,
    id_prefixes: &Vec<&str>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if id_prefixes.is_empty() {
        let index = find_id_prefix(config, None)?;
        return Ok(vec![config.wdeployments[index].id.clone()]);
    }
    let mut expansion = Vec::new();
    for id_prefix in id_prefixes.iter() {
        let index = find_id_prefix(config, Some(id_prefix))?;
        expansion.push(config.wdeployments[index].id.clone());
    }
    Ok(expansion)
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


pub fn get_username(token_path: &str) -> Result<String, Box<dyn std::error::Error>> {
    let token = std::fs::read(token_path)?;
    let token = String::from_utf8(token)?.trim().to_string();
    let claims = get_jwt_claims(&token)?;
    match claims["sub"].as_str() {
        Some(u) => Ok(u.into()),
        None => Err("token user not identified".into()),
    }
}


fn get_jwt_claims(rawtok: &str) -> Result<BTreeMap<String, serde_json::Value>, String> {
    let alg = PKeyWithDigest {
        digest: MessageDigest::sha256(),
        key: PKey::public_key_from_pem(PUBLIC_KEY).unwrap(),
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
            _ => Err(format!("error: {}", err)),
        },
    }
}


#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::find_id_prefix;
    use super::get_jwt_claims;
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


    #[test]
    fn detect_expired_token() {
        const EXPIRED_TOK: &str = "eyJ0eXAiOiJKV1QiLCJhbGciOiJSUzI1NiJ9.eyJvcmciOiJTdGFnaW5nVXNlclRlYW0iLCJzdWIiOiJzdGFnaW5nX3VzZXIiLCJpc3MiOiJyZXJvYm90cy5uZXQiLCJhdWQiOiJyZXJvYm90cy5uZXQiLCJleHAiOjE2NTg4NjgwMzIsIm5iZiI6MTY1ODgzMjAzMn0.Wq8vZ6XYs-pSmszcchXJPm3PnNGHtyM9ZktbjqMXgXl_TEdDrOH7HBYlYhoyNoyNwK4RkEqBxJLybH2qUmiSL7ljGIpKMhvpg6Rdytlx3tD7g__EeGusGO-4KrvCBGojTtSH4tm8jYxRmZVJXAfyqYqh3ZBickXwG-kWxNlz-vT3oAmVn4oSr5H0cf4WPS95uDo0X0j2nYroHyhHEuBIh2wy-8bcvolMweyKaa4Vo6h-bU4hiqQ3RHXJM7achzw_DIi3_eMVfJzsT1i1TovbCTNicUzwXGJcZJPBsQgU1KhD463rsv8N-o8o0oF3qU61n7oDQJGW8mbtzyFKIopTYZ3njWYZpkELS3ElKHVT92iVbOVlgGaicxxxFeg2Zz7fp6fFQWCZBWuoVwCguyoVG91XnEmk1Dlw7o9Bxmrgpmyyavg2A066CgV4b3YbrJaiOj1p8vITh3cTV2ca4iS2tUegYA1lEyJnmDPu09bdLC-hDR1MTBusu_jMOT7G1L_2z1a-SulgQbUBONU1387jgU6lr-1IoEZfYNVsdXCunqG6tcJXp-RXGQpekwm4wClBXpXGcYslYaIsMNnZrS_te43TYijkXiwZmp4wIFhmm9CcZNJ9vWFlw2KY5p5ilP4uE81a5LcM5jin4FdC1DE3qfJvN7hvYid80JfelsbopNE";
        assert_eq!(get_jwt_claims(EXPIRED_TOK), Err("expired".into()));
        let mut tok = String::from(EXPIRED_TOK);
        tok.push('F');
        assert_eq!(
            get_jwt_claims(tok.as_str()),
            Err("invalid signature".into())
        );
    }
}
