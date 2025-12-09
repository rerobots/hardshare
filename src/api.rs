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

use std::collections::HashMap;
use std::process;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use actix::io::SinkWrite;
use actix::prelude::*;
use actix_codec::Framed;
use actix_web::web::Bytes;
use awc::{
    error::WsProtocolError,
    ws::{Codec, Frame, Message},
    BoxedSocket,
};

use futures::stream::{SplitSink, StreamExt};

extern crate serde;
extern crate serde_json;
use serde::{Deserialize, Serialize};

use crate::camera;
use crate::control;
use crate::control::{CWorkerCommand, TunnelInfo};
use crate::mgmt;
use crate::mgmt::WDeployment;

struct ClientError {
    msg: String,
}
impl std::error::Error for ClientError {}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::fmt::Debug for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

pub fn error<T, S>(msg: S) -> Result<T, Box<dyn std::error::Error>>
where
    S: ToString,
{
    Err(Box::new(ClientError {
        msg: msg.to_string(),
    }))
}

#[derive(Serialize, Deserialize)]
pub struct AccessRule {
    capability: String,
    date_created: String,
    pub id: u16,
    param: Option<serde_json::Value>,
    pub user: String,
    pub wdeployment_id: String,
}

#[derive(Serialize, Deserialize)]
pub struct AccessRules {
    pub rules: Vec<AccessRule>,

    #[serde(default)]
    pub comment: Option<String>,
}

impl std::fmt::Display for AccessRules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            serde_yaml::to_string(self).expect("AccessRule can be serialized to YAML")
        )
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum AddOn {
    MistyProxy,
}

impl std::fmt::Display for AddOn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AddOn::MistyProxy => write!(f, "mistyproxy"),
        }
    }
}

#[derive(Clone)]
pub struct CameraDimensions {
    pub width: u32,
    pub height: u32,
}

impl std::str::FromStr for CameraDimensions {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut width_height = Vec::new();
        for x_raw in s.split(',') {
            let x: u32 = match x_raw.parse() {
                Ok(x) => x,
                Err(err) => return Err(format!("failed to parse width, height parameter: {err}")),
            };
            width_height.push(x);
        }
        if width_height.len() > 2 {
            return Err("too many values given for (width, height)".into());
        }
        if width_height.len() < 2 {
            return Err("missing values for (width, height)".into());
        }
        Ok(CameraDimensions {
            width: width_height[0],
            height: width_height[1],
        })
    }
}

pub type CameraCrop = HashMap<String, Vec<u16>>;

#[derive(Clone)]
pub struct HSAPIClient {
    local_config: Option<mgmt::Config>,
    cached_api_token: Option<String>,
    origin: String,
    wdid_tab: Option<HashMap<String, Addr<MainActor>>>,
}

#[derive(Serialize, Deserialize)]
pub struct DaemonStatus {
    ad_deployments: Vec<String>,
}

impl std::fmt::Display for DaemonStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "advertised deployments:")?;
        if self.ad_deployments.is_empty() {
            writeln!(f, "\t(none)")?;
        } else {
            for wd in self.ad_deployments.iter() {
                writeln!(f, "\t{wd}")?;
            }
        }
        Ok(())
    }
}

async fn get_access_rules_a(
    client: &awc::Client,
    origin: &str,
    wdid: &str,
) -> Result<AccessRules, Box<dyn std::error::Error>> {
    let url = format!("{origin}/deployment/{wdid}/rules");
    let mut resp = client.get(url).send().await?;
    if resp.status() == 200 {
        let payload: AccessRules = serde_json::from_slice(resp.body().await?.as_ref())?;
        Ok(payload)
    } else if resp.status() == 400 {
        let payload: serde_json::Value = serde_json::from_slice(resp.body().await?.as_ref())?;
        error(
            payload["error_message"]
                .as_str()
                .expect("Error message from api.rerobots.net should be a string"),
        )
    } else {
        error(format!(
            "error contacting core API server: {}",
            resp.status()
        ))
    }
}

impl HSAPIClient {
    pub fn new() -> HSAPIClient {
        #[cfg(test)]
        let origin = mockito::server_url();

        #[cfg(test)]
        let mut hsclient = HSAPIClient {
            local_config: None,
            cached_api_token: None,
            origin,
            wdid_tab: None,
        };

        #[cfg(not(test))]
        let origin = option_env!("REROBOTS_ORIGIN")
            .unwrap_or("https://api.rerobots.net")
            .to_string();

        #[cfg(not(test))]
        let mut hsclient = match mgmt::get_local_config(false, false) {
            Ok(local_config) => HSAPIClient {
                local_config: Some(local_config),
                cached_api_token: None,
                origin,
                wdid_tab: None,
            },
            Err(_) => {
                return HSAPIClient {
                    local_config: None,
                    cached_api_token: None,
                    origin,
                    wdid_tab: None,
                }
            }
        };

        if let Some(local_config) = &hsclient.local_config {
            let org_name = match &local_config.default_org {
                Some(default_org) => default_org.as_str(),
                None => "()",
            };
            if local_config.api_tokens.contains_key(org_name)
                && !local_config.api_tokens[org_name].is_empty()
            {
                let raw_tok = std::fs::read(&local_config.api_tokens[org_name][0]).unwrap();
                let tok = String::from_utf8(raw_tok).unwrap().trim().to_string();
                hsclient.cached_api_token = Some(tok);
            }
        }

        hsclient
    }

    fn reload_config(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let local_config = mgmt::get_local_config(false, false)?;
        self.local_config = Some(local_config);
        Ok(())
    }

    fn create_client_generator(
        &self,
    ) -> Result<impl FnOnce() -> awc::Client, Box<dyn std::error::Error>> {
        let api_token = match &self.cached_api_token {
            Some(tok) => tok.clone(),
            None => match &self.local_config {
                Some(local_config) => {
                    return match &local_config.default_org {
                        Some(default_org) => {
                            error(format!("No valid API tokens found for org {default_org}"))
                        }
                        None => error("No valid API tokens found (no default org)"),
                    }
                }
                None => return error("No valid API tokens found"),
            },
        };

        Ok(Box::new(move || {
            awc::Client::builder()
                .add_default_header(("Authorization", format!("Bearer {api_token}")))
                .finish()
        }))
    }

    pub fn get_remote_config(
        &self,
        include_dissolved: bool,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let listurl_path = if include_dissolved {
                "/hardshare/list?with_dissolved"
            } else {
                "/hardshare/list"
            };
            let url = format!("{origin}{listurl_path}");

            let client = client();
            let mut resp = client.get(url).send().await?;
            if resp.status() == 200 {
                Ok(serde_json::from_slice(resp.body().await?.as_ref())?)
            } else if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                error(String::from(payload["error_message"].as_str().expect(
                    "Error message from api.rerobots.net should be a string",
                )))
            } else {
                error(format!(
                    "error contacting core API server: {}",
                    resp.status()
                ))
            }
        })
    }

    pub fn get_access_rules(&self, wdid: &str) -> Result<AccessRules, Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            get_access_rules_a(&client(), &origin, &wdid).await
        })
    }

    pub fn drop_access_rules(&self, wdid: &str) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let client = client();
            let ruleset = get_access_rules_a(&client, &origin, &wdid).await?;
            for rule in ruleset.rules.iter() {
                let url = format!("{}/deployment/{}/rule/{}", origin, wdid, rule.id);
                let resp = client.delete(url).send().await?;
                if resp.status() != 200 {
                    return error(format!(
                        "error deleting rule {}: {}",
                        rule.id,
                        resp.status()
                    ));
                }
            }

            Ok(())
        })
    }

    pub fn add_access_rule(
        &self,
        wdid: &str,
        to_user: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let td = std::time::Duration::new(10, 0);
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let to_user = to_user.to_string();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let mut body = HashMap::new();
            body.insert("cap", "CAP_INSTANTIATE");
            body.insert("user", to_user.as_str());

            let url = format!("{origin}/deployment/{wdid}/rule");
            let client = client();
            let client_req = client.post(url).timeout(td);
            let mut resp = client_req.send_json(&body).await?;
            if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                return error(payload["error_message"].as_str().unwrap());
            } else if resp.status() == 404 {
                return error("not found".to_string());
            } else if resp.status() != 200 {
                return error(format!("server indicated error: {}", resp.status()));
            }

            Ok(())
        })
    }

    pub fn toggle_lockout(
        &self,
        wdid: &str,
        make_locked: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let client = client();
            let url = format!("{origin}/deployment/{wdid}/lockout");
            let resp = if make_locked {
                client.post(url).send().await?
            } else {
                client.delete(url).send().await?
            };
            if resp.status() != 200 {
                return error(format!("error changing lock-out: {}", resp.status()));
            }

            Ok(())
        })
    }

    pub fn send_alert(&self, wdid: &str, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let message = message.to_string();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let mut body = HashMap::new();
            body.insert("msg", message);

            let url = format!("{origin}/hardshare/alert/{wdid}");
            let client = client();
            let client_req = client.post(url);
            let mut resp = client_req.send_json(&body).await?;
            if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                return error(payload["error_message"].as_str().unwrap());
            } else if resp.status() == 404 {
                return error("not found".to_string());
            } else if resp.status() != 200 {
                return error(format!("server indicated error: {}", resp.status()));
            }

            Ok(())
        })
    }

    pub fn register_hook_emails(
        &self,
        wdid: &str,
        addr: Vec<&str>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let addr: Vec<String> = addr.iter().map(|x| x.to_string()).collect();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let mut body = HashMap::new();
            body.insert("emails", addr);

            let url = format!("{origin}/hardshare/hook/email/{wdid}");
            let client = client();
            let client_req = client.post(url);
            let mut resp = client_req.send_json(&body).await?;
            if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                return error(payload["error_message"].as_str().unwrap());
            } else if resp.status() == 404 {
                return error("not found".to_string());
            } else if resp.status() != 200 {
                return error(format!("server indicated error: {}", resp.status()));
            }

            Ok(())
        })
    }

    pub fn dissolve_wdeployment(&mut self, wdid: &str) -> Result<(), Box<dyn std::error::Error>> {
        let local_config = match &self.local_config {
            Some(local_config) => {
                if local_config.wdeployments.is_empty() {
                    return error("Unexpected dissolve request: local configuration is empty");
                }
                local_config
            }
            None => {
                return error("cannot dissolve without local configuration");
            }
        };

        let mut wd_index = None;
        for (j, wd) in local_config.wdeployments.iter().enumerate() {
            if wd.id == wdid {
                wd_index = Some(j);
                break;
            }
        }
        let wd_index = wd_index.unwrap();

        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let url = format!("{origin}/hardshare/dis/{wdid}");
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let client = client();

            let resp = client.post(url).send().await?;
            if resp.status() != 200 {
                return error(format!("error dissolving: {}", resp.status()));
            }

            Ok(())
        })?;

        if let Some(local_config) = &mut self.local_config {
            local_config.wdeployments.remove(wd_index);
            mgmt::modify_local(local_config)?;
        }
        Ok(())
    }

    pub fn get_addon_config(
        &self,
        wdid: &str,
        addon: &AddOn,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let addon = addon.clone();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let url = format!("{origin}/deployment/{wdid}");
            let client = client();
            let mut resp = client.get(url).send().await?;
            if resp.status() == 200 {
                let mut payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                let has_addon = payload["supported_addons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|x| x.as_str().unwrap() == addon.to_string());
                if !has_addon {
                    error(format!("add-on {addon} is not enabled"))
                } else {
                    Ok(payload["addons_config"][addon.to_string()].take())
                }
            } else if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                error(payload["error_message"].as_str().unwrap())
            } else {
                error(format!(
                    "error contacting core API server: {}",
                    resp.status()
                ))
            }
        })
    }

    pub fn remove_addon(
        &self,
        wdid: &str,
        addon: &AddOn,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let td = std::time::Duration::new(10, 0);
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let addon = addon.clone();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let url = format!("{origin}/deployment/{wdid}");
            let client = client();
            let mut resp = client.get(url).send().await?;
            if resp.status() == 200 {
                let mut payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                let mut supported_addons: Vec<String> = payload["supported_addons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| String::from(x.as_str().unwrap()))
                    .collect();
                let this_addon = addon.to_string();
                match supported_addons.iter().position(|x| x == &this_addon) {
                    Some(j) => {
                        supported_addons.remove(j);
                        let mut update_payload: HashMap<String, serde_json::Value> = HashMap::new();
                        if payload.as_object().unwrap().contains_key("addons_config") {
                            let mut addons_config = payload["addons_config"].take();
                            addons_config.as_object_mut().unwrap().remove(&this_addon);
                            update_payload.insert("addons_config".into(), addons_config);
                        }
                        update_payload.insert("supported_addons".into(), supported_addons.into());
                        let url = format!("{origin}/hardshare/wd/{wdid}");
                        let resp = client
                            .post(url)
                            .timeout(td)
                            .send_json(&update_payload)
                            .await?;
                        if resp.status() == 200 {
                            Ok(())
                        } else {
                            error(format!(
                                "error contacting hardshare server: {}",
                                resp.status()
                            ))
                        }
                    }
                    None => Ok(()),
                }
            } else if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                error(payload["error_message"].as_str().unwrap())
            } else {
                error(format!(
                    "error contacting core API server: {}",
                    resp.status()
                ))
            }
        })
    }

    fn is_locked_out(&self, wdid: &str) -> Result<bool, Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let url = format!("{origin}/deployment/{wdid}");
            let client = client();
            let mut resp = client.get(url).send().await?;
            if resp.status() == 200 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                Ok(payload["lockout"].as_bool().unwrap_or(false))
            } else if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                error(payload["error_message"].as_str().unwrap())
            } else {
                error(format!(
                    "error contacting core API server: {}",
                    resp.status()
                ))
            }
        })
    }

    fn upsert_addon(
        &self,
        wdid: &str,
        addon: &AddOn,
        config: Option<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let client = self.create_client_generator()?;
        let td = std::time::Duration::new(10, 0);
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let addon = addon.clone();
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let url = format!("{origin}/deployment/{wdid}");
            let client = client();
            let mut resp = client.get(url).send().await?;
            if resp.status() == 200 {
                let mut payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                let this_addon = addon.to_string();
                let mut supported_addons: Vec<String> = payload["supported_addons"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|x| String::from(x.as_str().unwrap()))
                    .collect();
                if !supported_addons.contains(&this_addon) {
                    supported_addons.push(this_addon.clone());
                }
                let mut update_payload: HashMap<String, serde_json::Value> = HashMap::new();
                update_payload.insert("supported_addons".into(), supported_addons.into());
                if payload.as_object().unwrap().contains_key("addons_config") {
                    let addons_config = payload["addons_config"].take();
                    update_payload.insert("addons_config".into(), addons_config);
                }
                if let Some(this_addon_config) = config {
                    match update_payload.get_mut("addons_config") {
                        Some(addonsc) => {
                            addonsc
                                .as_object_mut()
                                .unwrap()
                                .insert(this_addon, this_addon_config);
                        }
                        None => {
                            let addons_config = json!({
                                "addons_config": {
                                    this_addon: this_addon_config
                                }
                            });
                            update_payload.insert("addons_config".into(), addons_config);
                        }
                    }
                }

                let url = format!("{origin}/hardshare/wd/{wdid}");
                let resp = client
                    .post(url)
                    .timeout(td)
                    .send_json(&update_payload)
                    .await?;
                if resp.status() == 200 {
                    Ok(())
                } else {
                    error(format!(
                        "error contacting hardshare server: {}",
                        resp.status()
                    ))
                }
            } else if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                error(payload["error_message"].as_str().unwrap())
            } else {
                error(format!(
                    "error contacting core API server: {}",
                    resp.status()
                ))
            }
        })
    }

    pub fn add_mistyproxy(&self, wdid: &str, addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mistyproxy_config = json!({ "ip": addr });
        self.upsert_addon(wdid, &AddOn::MistyProxy, Some(mistyproxy_config))
    }

    pub fn stop(&self, wdid: &str, bindaddr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("http://{bindaddr}/stop/{wdid}");
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async {
            awc::Client::new()
                .post(url)
                .send()
                .await
                .or_else(error)
                .and_then(|resp| {
                    if resp.status() == 200 {
                        Ok(())
                    } else {
                        error(format!("{}", resp.status()))
                    }
                })
        })
    }

    async fn ad(
        ac: &Arc<Mutex<HSAPIClient>>,
        wdid: String,
    ) -> Result<Addr<MainActor>, Box<dyn std::error::Error>> {
        let authheader;
        let url;
        let wd;
        {
            let ac_inner = ac.lock().unwrap();
            authheader = format!("Bearer {}", &ac_inner.cached_api_token.as_ref().unwrap());
            url = format!("{}/hardshare/ad/{}", &ac_inner.origin, wdid);

            let local_config = &mut ac_inner.local_config.clone().unwrap();
            let wd_index = mgmt::find_id_prefix(local_config, Some(&wdid))?;
            local_config.wdeployments[wd_index].ssh_key = Some(local_config.ssh_key.clone());
            wd = Arc::new(local_config.wdeployments[wd_index].clone());
        }

        let (cworker_tx, cworker_rx) = mpsc::channel();
        let main_actor_addr = MainActor::create(|_ctx| MainActor {
            worker_req: cworker_tx,
            wsclient_addr: None,
        });

        let addr = open_websocket(&url, &authheader, &main_actor_addr, None)
            .await
            .unwrap();
        main_actor_addr.do_send(NewWS(Some(addr)));

        let ma_addr_for_cworker = main_actor_addr.clone();
        std::thread::spawn(move || control::cworker(cworker_rx, ma_addr_for_cworker, wd));

        Ok(main_actor_addr)
    }

    async fn http_post_reload_config(
        ac: actix_web::web::Data<Arc<Mutex<HSAPIClient>>>,
    ) -> actix_web::HttpResponse {
        let mut ac_inner = ac.lock().unwrap();
        match ac_inner.reload_config() {
            Ok(()) => actix_web::HttpResponse::Ok().finish(),
            Err(err) => {
                error!("{err}");
                actix_web::HttpResponse::InternalServerError().finish()
            }
        }
    }

    async fn http_post_start(
        wdid: actix_web::web::Path<String>,
        ac: actix_web::web::Data<Arc<Mutex<HSAPIClient>>>,
    ) -> actix_web::HttpResponse {
        let wdid_expanded;
        {
            let mut ac_inner = ac.lock().unwrap();
            wdid_expanded = match &ac_inner.local_config {
                Some(local_config) => {
                    let wd_index = match mgmt::find_id_prefix(local_config, Some(wdid.as_str())) {
                        Ok(wi) => wi,
                        Err(_) => return actix_web::HttpResponse::NotFound().finish(),
                    };
                    local_config.wdeployments[wd_index].id.clone()
                }
                None => {
                    warn!("start ad called when no local configuration");
                    return actix_web::HttpResponse::InternalServerError().finish();
                }
            };

            if let Some(wdid_tab) = &mut ac_inner.wdid_tab {
                if wdid_tab.contains_key(&*wdid_expanded) {
                    warn!(
                        "start ad called when already advertising {}",
                        &*wdid_expanded
                    );
                    return actix_web::HttpResponse::Forbidden().finish();
                }
            }
        }

        let addr = match HSAPIClient::ad(&ac, wdid_expanded.clone()).await {
            Ok(a) => a,
            Err(err) => {
                error!("{err}");
                return actix_web::HttpResponse::InternalServerError().finish();
            }
        };

        {
            let mut ac_inner = ac.lock().unwrap();
            if let Some(wdid_tab) = &mut ac_inner.wdid_tab {
                wdid_tab.insert(wdid_expanded.clone(), addr);
            }
        }

        actix_web::HttpResponse::Ok().finish()
    }

    async fn http_post_stop(
        wdid: actix_web::web::Path<String>,
        ac: actix_web::web::Data<Arc<Mutex<HSAPIClient>>>,
    ) -> actix_web::HttpResponse {
        let mut ac_inner = ac.lock().unwrap();
        if let Some(wdid_tab) = &mut ac_inner.wdid_tab {
            match wdid_tab.remove(&*wdid) {
                Some(addr) => {
                    if wdid_tab.is_empty() {
                        addr.do_send(MainActorCommand("STOP DAEMON".into()));
                    } else {
                        addr.do_send(MainActorCommand("STOP".into()));
                    }
                    actix_web::HttpResponse::Ok().finish()
                }
                None => actix_web::HttpResponse::NotFound().finish(),
            }
        } else {
            actix_web::HttpResponse::InternalServerError().finish()
        }
    }

    async fn http_get_status(
        ac: actix_web::web::Data<Arc<Mutex<HSAPIClient>>>,
    ) -> actix_web::HttpResponse {
        let mut daemon_status = DaemonStatus {
            ad_deployments: vec![],
        };
        let ac_inner = ac.lock().unwrap();
        if let Some(wdid_tab) = &ac_inner.wdid_tab {
            for k in wdid_tab.keys() {
                daemon_status.ad_deployments.push(k.clone());
            }
        }
        actix_web::HttpResponse::Ok().json(daemon_status)
    }

    pub fn run(&self, wdid: &str, bindaddr: &str) -> Result<(), Box<dyn std::error::Error>> {
        if self.cached_api_token.is_none() {
            return error("No valid API tokens found.");
        }

        match self.is_locked_out(wdid) {
            Ok(locked_out) => {
                if locked_out {
                    warn!("advertised deployment {wdid} is locked out");
                    warn!("new instances will be rejected until unlock");
                }
            }
            Err(err) => {
                error!("failed to determine whether deployment is locked out");
                return Err(err);
            }
        }

        // Try to start via daemon, if exists
        let url = format!("http://{bindaddr}/start/{wdid}");
        let sys = System::new();
        let res = actix::SystemRunner::block_on(&sys, async {
            awc::Client::new().post(url).send().await
        });
        match res {
            Ok(res) => {
                if res.status() == 403 {
                    warn!("ignoring because daemon already advertising {wdid}");
                } else {
                    info!("started via existing daemon");
                }
                return Ok(());
            }
            Err(err) => info!("no existing daemon: {err}"),
        };

        // Else, start new daemon
        info!("starting new daemon");
        let bindaddr: std::net::SocketAddr = bindaddr.parse()?;
        let wdid = String::from(wdid);

        let sys = System::new();
        let (err_notify, err_rx) = mpsc::channel();
        let ac = Arc::new(Mutex::new(self.clone()));
        sys.runtime().spawn(async move {
            let addr = match HSAPIClient::ad(&ac, wdid.clone()).await {
                Ok(a) => a,
                Err(err) => {
                    err_notify.send(format!("{err}")).unwrap();
                    System::current().stop_with_code(1);
                    return;
                }
            };
            let mut wdid_tab = HashMap::new();
            wdid_tab.insert(wdid.clone(), addr.clone());
            {
                let mut ac_inner = ac.lock().unwrap();
                ac_inner.wdid_tab = Some(wdid_tab);
            }

            let mut manip = actix_web::HttpServer::new(move || {
                let ac = Arc::clone(&ac);
                actix_web::App::new()
                    .app_data(ac)
                    .wrap(actix_web::middleware::Logger::default())
                    .route(
                        "/status",
                        actix_web::web::get().to(HSAPIClient::http_get_status),
                    )
                    .route(
                        "/stop/{wdid:.*}",
                        actix_web::web::post().to(HSAPIClient::http_post_stop),
                    )
                    .route(
                        "/start/{wdid:.*}",
                        actix_web::web::post().to(HSAPIClient::http_post_start),
                    )
                    .route(
                        "/reload",
                        actix_web::web::post().to(HSAPIClient::http_post_reload_config),
                    )
            })
            .workers(1);
            manip = match manip.bind(bindaddr) {
                Ok(s) => s,
                Err(err) => {
                    err_notify
                        .send(format!("failed to bind to {bindaddr}; {err}"))
                        .unwrap();
                    System::current().stop_with_code(1);
                    return;
                }
            };
            match manip.run().await {
                Ok(()) => (),
                Err(err) => {
                    err_notify
                        .send(format!("failed to start listener: {err}"))
                        .unwrap();
                    System::current().stop_with_code(1);
                }
            }
        });
        match sys.run() {
            Ok(()) => Ok(()),
            Err(_) => error(err_rx.recv()?),
        }
    }

    pub fn get_local_status(
        &self,
        bindaddr: &str,
    ) -> Result<DaemonStatus, Box<dyn std::error::Error>> {
        let url = format!("http://{bindaddr}/status");
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async {
            let mut resp = awc::Client::new().get(url).send().await?;
            if resp.status() == 200 {
                let r: DaemonStatus = serde_json::from_slice(resp.body().await?.as_ref())?;
                Ok(r)
            } else {
                error(format!("error contacting daemon: {}", resp.status()))
            }
        })
    }

    pub fn req_reload_config(&self, bindaddr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("http://{bindaddr}/reload");
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async {
            let resp = awc::Client::new().post(url).send().await?;
            if resp.status() == 200 {
                Ok(())
            } else {
                error(format!("error contacting daemon: {}", resp.status()))
            }
        })
    }

    pub fn register_new(&mut self, at_most_1: bool) -> Result<String, Box<dyn std::error::Error>> {
        let local_config = match &mut self.local_config {
            Some(local_config) => {
                if at_most_1 && !local_config.wdeployments.is_empty() {
                    return error("local configuration already declares a workspace deployment (to register more, `hardshare register --permit-more`)");
                }
                local_config
            }
            None => {
                return error("cannot register without initial local configuration. (try `hardshare config --create`)");
            }
        };

        let url = format!("{}/hardshare/register", self.origin);
        let authheader = format!("Bearer {}", self.cached_api_token.as_ref().unwrap());

        let sys = System::new();
        let res = actix::SystemRunner::block_on(&sys, async {
            let client = awc::Client::builder()
                .add_default_header(("Authorization", authheader))
                .finish();
            let mut resp = client.post(url).send().await?;
            if resp.status() == 200 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                let mut new_wd = HashMap::new();
                new_wd.insert("id".into(), json!(payload["id"].as_str().unwrap()));
                new_wd.insert("owner".into(), json!(payload["owner"].as_str().unwrap()));
                Ok(new_wd)
            } else if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                error(String::from(payload["error_message"].as_str().unwrap()))
            } else {
                error(format!("server indicated error: {}", resp.status()))
            }
        });
        let new_wd = res?;

        local_config
            .wdeployments
            .push(WDeployment::from_json(&new_wd));

        #[cfg(not(test))]
        mgmt::modify_local(local_config)?;

        Ok(local_config.wdeployments.last().unwrap().id.clone())
    }

    pub fn declare_existing(&mut self, wdid: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(local_config) = &self.local_config {
            for wd in local_config.wdeployments.iter() {
                if wd.id == wdid {
                    return error("attempted to declare workspace deployment that is already declared in local configuration");
                }
            }
        } else {
            return error("cannot declare existing without initial local configuration. (try `hardshare config --create`)");
        }

        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let url = format!("{origin}/hardshare/list");
        let sys = System::new();

        let wdid = wdid.to_string();
        let res = actix::SystemRunner::block_on(&sys, async move {
            let client = client();
            let mut resp = client.get(url).send().await?;
            if resp.status() == 200 {
                let body = resp.body().await?;
                let parsed_body: serde_json::Value = serde_json::from_slice(body.as_ref())?;
                for wd in parsed_body["wdeployments"].as_array().unwrap().iter() {
                    if wd["id"].as_str().unwrap() == wdid {
                        let mut matched_wd: HashMap<String, serde_json::Value> = HashMap::new();
                        matched_wd.insert("id".into(), json!(wd["id"].as_str().unwrap()));
                        matched_wd.insert(
                            "owner".into(),
                            json!(parsed_body["owner"].as_str().unwrap()),
                        );
                        return Ok(Some(matched_wd));
                    }
                }
                Ok(None)
            } else if resp.status() == 400 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                error(String::from(payload["error_message"].as_str().unwrap()))
            } else {
                error(format!(
                    "error contacting core API server: {}",
                    resp.status()
                ))
            }
        });

        let matched_wd = match res? {
            Some(matched_wd) => matched_wd,
            None => {
                return error("no previously registered workspace deployments found with given ID")
            }
        };

        if let Some(local_config) = &mut self.local_config {
            local_config
                .wdeployments
                .push(WDeployment::from_json(&matched_wd));

            #[cfg(not(test))]
            mgmt::modify_local(local_config)?;
        }
        Ok(())
    }

    pub fn attach_camera(
        &self,
        camera_path: &str,
        wds: &Vec<String>,
        dim: &Option<CameraDimensions>,
        crop: &Option<CameraCrop>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let api_token = self.cached_api_token.as_ref().ok_or("no valid API token")?;
        let client = self.create_client_generator()?;
        let td = std::time::Duration::new(10, 0);
        let origin = self.origin.clone();

        let mut opts = json!({ "wds": wds });
        if let Some(crop) = crop {
            opts["crop"] = json!(crop);
        }

        let sys = System::new();
        let res = actix::SystemRunner::block_on(&sys, async move {
            let client = client();
            let url = format!("{origin}/hardshare/cam");
            let client_req = client.post(url).timeout(td);
            let mut resp = client_req.send_json(&opts).await?;
            if resp.status() == 200 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;
                Ok(payload["id"].as_str().unwrap().to_string())
            } else {
                let mut err = format!("server indicated error: status {}", resp.status());
                if resp.headers().contains_key("content-length")
                    && resp.headers().get("content-length").unwrap() != "0"
                {
                    err += &format!(
                        "; body: {}",
                        String::from_utf8_lossy(resp.body().await?.as_ref())
                    );
                }
                error(err)
            }
        });
        let hscamera_id = res?;
        debug!("registered new hscamera: {hscamera_id}");

        let base_path = mgmt::get_base_path().unwrap();
        let path = base_path.join("camera");
        if !path.exists() {
            std::fs::create_dir(&path)?
        }
        let path = path.join(format!("{hscamera_id}.pid"));
        let pid = process::id();
        std::fs::write(&path, pid.to_string())?;

        debug!("starting camera stream...");
        let exit_result =
            camera::stream_websocket(&self.origin, api_token, &hscamera_id, camera_path, dim);

        if exit_result.is_err() {
            std::fs::remove_file(path)?;
            let client = self.create_client_generator()?;
            let origin = self.origin.clone();
            actix::SystemRunner::block_on(&sys, async move {
                let client = client();
                let url = format!("{origin}/hardshare/cam/{hscamera_id}");
                let resp = client.delete(url).send().await?;
                if resp.status() != 200 {
                    return error(format!(
                        "error stopping camera {}: {}",
                        hscamera_id,
                        resp.status()
                    ));
                }
                Ok(())
            })?;
        }

        exit_result
    }

    pub fn stop_cameras(&self, all: bool, force: bool) -> Result<(), Box<dyn std::error::Error>> {
        let base_path = mgmt::get_base_path().unwrap();
        let path = base_path.join("camera");
        let mut stopped_via_pids = Vec::new();
        if path.exists() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    continue;
                }
                let entry = entry.path();

                if entry.extension().unwrap() == "pid" {
                    let file_stem = entry.file_stem().unwrap();
                    stopped_via_pids.push(file_stem.to_string_lossy().to_string());
                    let pid = String::from_utf8(std::fs::read(&entry).unwrap())
                        .unwrap()
                        .trim()
                        .to_string();

                    #[cfg(target_os = "windows")]
                    let kresult = process::Command::new("taskkill")
                        .args(["/pid", &pid])
                        .status();
                    #[cfg(any(target_os = "linux", target_os = "macos"))]
                    let kresult = process::Command::new("kill").arg(&pid).status();

                    match kresult {
                        Ok(r) => {
                            if !r.success() {
                                let msg = format!(
                                    "failed to terminate local process {} for camera {}: {}",
                                    pid,
                                    stopped_via_pids.last().unwrap(),
                                    r
                                );
                                if !force {
                                    return error(msg);
                                } else {
                                    warn!("{msg}");
                                }
                            }
                        }
                        Err(err) => {
                            let msg = format!(
                                "failed to terminate local process {} for camera {}: {}",
                                pid,
                                stopped_via_pids.last().unwrap(),
                                err
                            );
                            if !force {
                                return error(msg);
                            } else {
                                warn!("{msg}");
                            }
                        }
                    }

                    std::fs::remove_file(entry)?;
                }
            }
        }

        let local_wdeployments = match &self.local_config {
            Some(c) => c.wdeployments.iter().map(|x| x.id.clone()).collect(),
            None => vec![],
        };

        let client = self.create_client_generator()?;
        let origin = self.origin.clone();
        let url = format!("{origin}/hardshare/cam");
        let sys = System::new();
        actix::SystemRunner::block_on(&sys, async move {
            let client = client();
            let mut resp = client.get(url).send().await?;
            if resp.status() == 200 {
                let payload: serde_json::Value =
                    serde_json::from_slice(resp.body().await?.as_ref())?;

                let hscameras = payload.as_object().unwrap();
                debug!("{hscameras:?}");
                for (hscamera_id, assoc) in hscameras.iter() {
                    debug!("{hscamera_id:?}: {assoc:?}");
                    if !all {
                        if !stopped_via_pids.iter().any(|x| x == hscamera_id) {
                            continue;
                        }

                        let assoc: Vec<String> = assoc
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|x| x.as_str().unwrap().to_string())
                            .collect();

                        if !nonempty_intersection(&local_wdeployments, &assoc) {
                            continue;
                        }
                    }

                    let url = format!("{origin}/hardshare/cam/{hscamera_id}");
                    let resp = client.delete(url).send().await?;
                    if resp.status() != 200 {
                        return error(format!(
                            "error stopping camera {}: {}",
                            hscamera_id,
                            resp.status()
                        ));
                    }
                }
                Ok(())
            } else {
                error(format!("error enumerating cameras: {}", resp.status()))
            }
        })
    }
}

// Try at least once, independent of timeout
async fn open_websocket(
    url: &str,
    authheader: &str,
    main_actor_addr: &Addr<MainActor>,
    timeout: Option<Duration>,
) -> Result<Addr<WSClient>, Box<dyn std::error::Error>> {
    let sleep_time = std::time::Duration::from_secs(1);
    let now = std::time::Instant::now();

    loop {
        let authheader_dup = String::from(authheader);
        let url_dup = String::from(url);
        let client = awc::Client::builder()
            .add_default_header(("Authorization", authheader))
            .finish();

        let (_, framed) = match client.ws(url).connect().await {
            Ok(c) => c,
            Err(err) => {
                if timeout.is_some() && Some(now.elapsed()) > timeout {
                    return Err(Box::new(err));
                } else {
                    warn!("failed to open WebSocket: {err}");
                    std::thread::sleep(sleep_time);
                    continue;
                }
            }
        };
        let (sink, stream) = framed.split();

        let ma_addr_for_wsclient = main_actor_addr.clone();

        return Ok(WSClient::create(|ctx| {
            WSClient::add_stream(stream, ctx);
            WSClient {
                ws_url: url_dup,
                ws_auth: authheader_dup,
                ws_sink: SinkWrite::new(sink, ctx),
                recent_rx_instant: std::time::Instant::now(), // First instant at first connect
                main_actor_addr: ma_addr_for_wsclient,
            }
        }));
    }
}

pub struct WSClient {
    ws_url: String,
    ws_auth: String,
    ws_sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    recent_rx_instant: std::time::Instant,
    main_actor_addr: Addr<MainActor>,
}

#[derive(Message)]
#[rtype(result = "()")]
struct WSSend(String);

impl Actor for WSClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.check_receive_timeout(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        debug!("WSClient actor stopped");
    }
}

impl WSClient {
    fn check_receive_timeout(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(60, 0), |act, ctx| {
            if act.recent_rx_instant.elapsed() > Duration::new(45, 0) {
                debug!("timeout waiting for server");
                match act.ws_sink.write(Message::Close(None)) {
                    Ok(()) => (),
                    Err(err) => error!("caught while attempting to close WebSocket: {err:?}"),
                };
                ctx.stop();
            } else {
                act.check_receive_timeout(ctx);
            }
        });
    }
}

impl Handler<WSSend> for WSClient {
    type Result = ();

    fn handle(&mut self, msg: WSSend, _ctx: &mut Context<Self>) {
        match self.ws_sink.write(Message::Text(msg.0.into())) {
            Ok(()) => (),
            Err(err) => error!("caught while attempting to send message via WebSocket: {err:?}"),
        }
    }
}

impl StreamHandler<Result<Frame, WsProtocolError>> for WSClient {
    fn handle(&mut self, msg: Result<Frame, WsProtocolError>, _ctx: &mut Context<Self>) {
        self.recent_rx_instant = std::time::Instant::now();

        if let Ok(Frame::Text(txt)) = msg {
            let payload: serde_json::Value = match serde_json::from_slice(txt.as_ref()) {
                Ok(p) => p,
                Err(err) => {
                    error!("failed to parse {txt:?}: {err}");
                    return;
                }
            };
            debug!("received: {}", serde_json::to_string(&payload).unwrap());

            let message_ver = match payload["v"].as_i64() {
                Some(v) => v,
                None => {
                    error!("received message with no version declaration");
                    return;
                }
            };
            if message_ver != 0 {
                error!("received message of unknown format version: {message_ver}");
                return;
            }

            let cmd = match payload["cmd"].as_str() {
                Some(c) => c,
                None => {
                    error!("received message without `cmd` field");
                    return;
                }
            };

            let m = match cmd {
                "INSTANCE_LAUNCH" => CWorkerCommand::launch_instance(
                    payload["id"].as_str().unwrap(),
                    payload["mi"].as_str().unwrap(),
                    control::ConnType::SshTun,
                    payload["pr"].as_str().unwrap(),
                    payload["repo"].as_str(),
                    payload["repo_path"].as_str(),
                ),
                "INSTANCE_STATUS" => CWorkerCommand::get_status(
                    payload["id"].as_str().unwrap(),
                    payload["mi"].as_str().unwrap(),
                ),
                "INSTANCE_DESTROY" => CWorkerCommand::destroy_instance(
                    payload["id"].as_str().unwrap(),
                    payload["mi"].as_str().unwrap(),
                ),
                "CREATE_SSHTUN_DONE" => {
                    let tunnelinfo: TunnelInfo = match serde_json::from_slice(txt.as_ref()) {
                        Ok(x) => {
                            debug!("received tunnel info: {x:?}");
                            x
                        }
                        Err(err) => {
                            error!("failed to parse tunnel info from {txt:?}: {err}");
                            return;
                        }
                    };
                    CWorkerCommand::create_sshtun_done(
                        payload["id"].as_str().unwrap(),
                        payload["mi"].as_str().unwrap(),
                        &tunnelinfo,
                    )
                }
                _ => {
                    error!("unknown command: {cmd}");
                    return;
                }
            };
            self.main_actor_addr.do_send(ClientCommand(m));
        } else if let Ok(Frame::Ping(_)) = msg {
            debug!("received PING; sending PONG");
            match self.ws_sink.write(Message::Pong(Bytes::from_static(b""))) {
                Ok(()) => (),
                Err(err) => error!("caught while responding to WebSocket ping: {err:?}"),
            }
        } else {
            warn!("unrecognized WebSocket message: {msg:?}");
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        self.ws_sink.close();

        let authheader = self.ws_auth.clone();
        let url = self.ws_url.clone();
        let main_actor_addr = self.main_actor_addr.clone();
        let sys = System::new();
        sys.runtime().spawn(async move {
            main_actor_addr.do_send(NewWS(None));
            let addr = open_websocket(&url, &authheader, &main_actor_addr, None)
                .await
                .unwrap();
            main_actor_addr.do_send(NewWS(Some(addr)));
        });

        ctx.stop()
    }
}

impl actix::io::WriteHandler<WsProtocolError> for WSClient {}

pub struct MainActor {
    worker_req: mpsc::Sender<CWorkerCommand>,
    wsclient_addr: Option<Addr<WSClient>>,
}

impl Actor for MainActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Context<Self>) {
        debug!("MainActor started");
    }

    fn stopped(&mut self, _ctx: &mut Context<Self>) {
        debug!("MainActor stopped");
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct MainActorCommand(String);

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub struct ClientWorkerMessage {
    pub mtype: control::CWorkerMessageType,
    pub body: Option<String>,
}

#[derive(Message)]
#[rtype(result = "()")]
struct NewWS(Option<Addr<WSClient>>);

impl Handler<NewWS> for MainActor {
    type Result = ();

    fn handle(&mut self, msg: NewWS, _ctx: &mut Context<Self>) {
        match msg.0 {
            Some(ws) => {
                info!("new WebSocket");
                self.wsclient_addr = Some(ws);
            }
            None => {
                info!("closed WebSocket");
                self.wsclient_addr = None;
            }
        }
    }
}

impl Handler<MainActorCommand> for MainActor {
    type Result = ();

    fn handle(&mut self, msg: MainActorCommand, ctx: &mut Context<Self>) {
        debug!("received client command: {}", msg.0);
        if msg.0 == "STOP" {
            ctx.stop();
        } else if msg.0 == "STOP DAEMON" {
            ctx.stop();
            System::current().stop();
        } else if msg.0 == "RESTART WEBSOCKET" {
            self.wsclient_addr = None;
        } else {
            warn!("unknown client command: {}", msg.0);
        }
    }
}

impl Handler<ClientWorkerMessage> for MainActor {
    type Result = ();

    fn handle(&mut self, msg: ClientWorkerMessage, _ctx: &mut Context<Self>) {
        debug!("received client worker message: {msg:?}");
        match msg.mtype {
            control::CWorkerMessageType::WsSend => match &self.wsclient_addr {
                Some(wa) => {
                    wa.do_send(WSSend(msg.body.unwrap()));
                }
                None => {
                    error!("received WsSend when no WSClient");
                }
            },
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct ClientCommand(CWorkerCommand);

impl Handler<ClientCommand> for MainActor {
    type Result = ();

    fn handle(&mut self, msg: ClientCommand, _ctx: &mut Context<Self>) {
        self.worker_req.send(msg.0).unwrap();
    }
}

fn nonempty_intersection<T>(u: &[T], v: &[T]) -> bool
where
    T: PartialEq,
{
    for x in u.iter() {
        for y in v.iter() {
            if x == y {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use mockito::mock;

    use super::mgmt;
    use super::AddOn;
    use super::HSAPIClient;

    #[test]
    fn list_no_rules() {
        let wdid = "68a1be97-9365-4007-b726-14c56bd69eef";
        let path = format!("/deployment/{wdid}/rules");
        let _m = mock("GET", path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"rules": []}"#)
            .create();

        let mut ac = HSAPIClient::new();
        ac.cached_api_token = Some("fake".to_string());
        let ruleset = ac.get_access_rules(wdid).unwrap();

        assert_eq!(ruleset.rules.len(), 0)
    }

    #[test]
    fn get_mistyproxy_config() {
        let wdid = "68a1be97-9365-4007-b726-14c56bd69eef";
        let path = format!("/deployment/{wdid}");
        let addr = "192.168.1.7";
        let payload = json!({
            "supported_addons": ["mistyproxy"],
            "addons_config": {
                "mistyproxy": {
                    "ip": addr
                }
            }
        });
        let _m = mock("GET", path.as_str())
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(payload.to_string())
            .create();

        let mut ac = HSAPIClient::new();
        ac.cached_api_token = Some("fake".to_string());
        let addonsc = ac.get_addon_config(wdid, &AddOn::MistyProxy).unwrap();

        assert!(addonsc.as_object().unwrap().contains_key("ip"));
        let returned_addr = addonsc["ip"].as_str().unwrap();
        assert_eq!(addr, returned_addr);
    }

    #[test]
    fn register_new() {
        let expected_new_wdids = [
            "68a1be97-9365-4007-b726-14c56bd69eef",
            "2d6039bc-7c83-4d46-8567-c8df4711c386",
        ];

        let path = "/hardshare/register";
        let expected_res: Vec<serde_json::Value> = expected_new_wdids
            .iter()
            .map(|wdid| {
                json!({
                    "id": wdid,
                    "owner": "scott"
                })
            })
            .collect();
        let _m = mock("POST", path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(expected_res[0].to_string())
            .create();
        let _m2 = mock("POST", path)
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(expected_res[1].to_string())
            .create();

        let mut ac = HSAPIClient::new();
        ac.cached_api_token = Some("fake".to_string());
        ac.local_config = Some(mgmt::Config::new());
        let res = ac.register_new(true).unwrap();
        assert_eq!(res, expected_new_wdids[0]);

        let res = ac.register_new(true);
        assert!(res.is_err());

        let res = ac.register_new(false);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), expected_new_wdids[1]);
        assert_eq!(ac.local_config.unwrap().wdeployments.len(), 2);
    }
}
