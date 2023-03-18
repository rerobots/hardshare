// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use actix::io::SinkWrite;
use actix::prelude::*;
use actix_codec::Framed;
use awc::{
    error::WsProtocolError,
    ws::{Codec, Frame, Message},
    BoxedSocket,
};

use bytes::Bytes;
use futures::stream::{SplitSink, StreamExt};

use openssl::ssl::{SslConnector, SslMethod};

extern crate serde;
extern crate serde_json;
use serde::{Deserialize, Serialize};

use crate::mgmt;


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

fn error<T, S>(msg: S) -> Result<T, Box<dyn std::error::Error>>
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
        write!(f, "{}", serde_yaml::to_string(self).unwrap())
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
pub struct HSAPIClient {
    local_config: Option<mgmt::Config>,
    cached_api_token: Option<String>,
    origin: String,
    wdid_tab: Option<HashMap<String, Addr<WSClient>>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemoteConfig {}


fn create_client(token: Option<String>) -> Result<awc::Client, Box<dyn std::error::Error>> {
    if token.is_none() {
        return error("No valid API tokens found.");
    }

    let connector = SslConnector::builder(SslMethod::tls())?.build();
    let client = awc::Client::builder().connector(awc::Connector::new().ssl(connector).finish());
    Ok(client
        .header("Authorization", format!("Bearer {}", token.unwrap()))
        .finish())
}


async fn get_access_rules_a(
    client: &awc::Client,
    origin: &str,
    wdid: &str,
) -> Result<AccessRules, Box<dyn std::error::Error>> {
    let url = format!("{}/deployment/{}/rules", origin, wdid);
    let mut resp = client.get(url).send().await?;
    if resp.status() == 200 {
        let payload: AccessRules = serde_json::from_slice(resp.body().await?.as_ref())?;
        Ok(payload)
    } else if resp.status() == 400 {
        let payload: serde_json::Value = serde_json::from_slice(resp.body().await?.as_ref())?;
        error(payload["error_message"].as_str().unwrap())
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


    pub fn get_remote_config(
        &self,
        include_dissolved: bool,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let api_token = self.cached_api_token.clone();
        let origin = self.origin.clone();
        let mut sys = System::new("wclient");
        actix::SystemRunner::block_on(&mut sys, async move {
            let listurl_path = if include_dissolved {
                "/hardshare/list?with_dissolved"
            } else {
                "/hardshare/list"
            };
            let url = format!("{}{}", origin, listurl_path);

            let client = create_client(api_token)?;

            let mut resp = client.get(url).send().await?;
            if resp.status() == 200 {
                Ok(serde_json::from_slice(resp.body().await?.as_ref())?)
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
        })
    }


    pub fn get_access_rules(&self, wdid: &str) -> Result<AccessRules, Box<dyn std::error::Error>> {
        let api_token = self.cached_api_token.clone();
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let mut sys = System::new("wclient");
        actix::SystemRunner::block_on(&mut sys, async move {
            let client = create_client(api_token)?;
            get_access_rules_a(&client, &origin, &wdid).await
        })
    }


    pub fn drop_access_rules(&self, wdid: &str) -> Result<(), Box<dyn std::error::Error>> {
        let api_token = self.cached_api_token.clone();
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let mut sys = System::new("wclient");
        actix::SystemRunner::block_on(&mut sys, async move {
            let client = create_client(api_token)?;

            let ruleset = get_access_rules_a(&client, &origin, &wdid).await?;
            for rule in ruleset.rules.iter() {
                let url = format!("{}/deployment/{}/rule/{}", origin, wdid, rule.id);
                let mut resp = client.delete(url).send().await?;
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
        let td = std::time::Duration::new(10, 0);
        let api_token = self.cached_api_token.clone();
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let origin = self.origin.clone();
        let to_user = to_user.to_string();
        let mut sys = System::new("wclient");
        actix::SystemRunner::block_on(&mut sys, async move {
            let mut body = HashMap::new();
            body.insert("cap", "CAP_INSTANTIATE");
            body.insert("user", to_user.as_str());

            let client = create_client(api_token)?;

            let url = format!("{}/deployment/{}/rule", origin, wdid);
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


    pub fn get_addon_config(
        &self,
        wdid: &str,
        addon: &AddOn,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let api_token = self.cached_api_token.clone();
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let addon = addon.clone();
        let mut sys = System::new("wclient");
        actix::SystemRunner::block_on(&mut sys, async move {
            let client = create_client(api_token)?;
            let url = format!("{}/deployment/{}", origin, wdid);
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
                    error(format!("add-on {} is not enabled", addon))
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
        let td = std::time::Duration::new(10, 0);
        let api_token = self.cached_api_token.clone();
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let addon = addon.clone();
        let mut sys = System::new("wclient");
        actix::SystemRunner::block_on(&mut sys, async move {
            let client = create_client(api_token)?;
            let url = format!("{}/deployment/{}", origin, wdid);
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
                        let url = format!("{}/hardshare/wd/{}", origin, wdid);
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


    fn upsert_addon(
        &self,
        wdid: &str,
        addon: &AddOn,
        config: Option<serde_json::Value>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let td = std::time::Duration::new(10, 0);
        let api_token = self.cached_api_token.clone();
        let origin = self.origin.clone();
        let wdid = wdid.to_string();
        let addon = addon.clone();
        let mut sys = System::new("wclient");
        actix::SystemRunner::block_on(&mut sys, async move {
            let client = create_client(api_token)?;
            let url = format!("{}/deployment/{}", origin, wdid);
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
                    let mut addons_config = payload["addons_config"].take();
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

                let url = format!("{}/hardshare/wd/{}", origin, wdid);
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
        let url = format!("http://{}/stop/{}", bindaddr, wdid);
        let mut sys = System::new("dclient");
        actix::SystemRunner::block_on(&mut sys, async {
            awc::Client::new()
                .post(url)
                .send()
                .await
                .map(|resp| ())
                .or_else(error)
        })
    }


    async fn ad(
        ac: &HSAPIClient,
        wdid: String,
    ) -> Result<Addr<WSClient>, Box<dyn std::error::Error>> {
        let authheader = format!("Bearer {}", ac.cached_api_token.as_ref().unwrap());
        let url = format!("{}/hardshare/ad/{}", ac.origin, wdid);
        let connector = SslConnector::builder(SslMethod::tls())?.build();

        let client = awc::Client::builder()
            .connector(awc::Connector::new().ssl(connector).finish())
            .header("Authorization", authheader)
            .finish();

        let (_, framed) = client.ws(url).connect().await?;
        let (sink, stream) = framed.split();

        let (cworker_tx, cworker_rx) = mpsc::channel();
        let addr = WSClient::create(|ctx| {
            WSClient::add_stream(stream, ctx);
            WSClient {
                worker_req: cworker_tx,
                ws_sink: SinkWrite::new(sink, ctx),
                recent_rx_instant: std::time::Instant::now(), // First instant at first connect
            }
        });

        let ws_addr = addr.clone();
        let ac = ac.clone();
        std::thread::spawn(move || cworker(ac, cworker_rx, ws_addr));

        Ok(addr)
    }


    async fn http_post_start(
        wdid: actix_web::web::Path<String>,
        ac: actix_web::web::Data<Arc<Mutex<HSAPIClient>>>,
    ) -> actix_web::HttpResponse {
        let mut ac_inner = ac.lock().unwrap();
        if let Some(local_config) = &ac_inner.local_config {
            let wd_index = match mgmt::find_id_prefix(local_config, Some(wdid.as_str())) {
                Ok(wi) => wi,
                Err(err) => return actix_web::HttpResponse::NotFound().finish(),
            };
        }

        if let Some(wdid_tab) = &mut (*ac_inner).wdid_tab {
            if wdid_tab.contains_key(&*wdid) {
                warn!("start ad called when already advertising {}", &*wdid);
                return actix_web::HttpResponse::Forbidden().finish();
            }
        }

        let addr = match HSAPIClient::ad(&*ac_inner, wdid.clone()).await {
            Ok(a) => a,
            Err(err) => {
                error!("{}", err);
                return actix_web::HttpResponse::InternalServerError().finish();
            }
        };

        if let Some(wdid_tab) = &mut (*ac_inner).wdid_tab {
            wdid_tab.insert(wdid.clone(), addr);
        }

        actix_web::HttpResponse::Ok().finish()
    }


    fn http_post_stop(
        wdid: actix_web::web::Path<String>,
        ac: actix_web::web::Data<Arc<Mutex<HSAPIClient>>>,
    ) -> actix_web::HttpResponse {
        let mut ac_inner = ac.lock().unwrap();
        if let Some(wdid_tab) = &mut (*ac_inner).wdid_tab {
            match wdid_tab.remove(&*wdid) {
                Some(addr) => {
                    if wdid_tab.is_empty() {
                        addr.do_send(WSClientCommand("STOP DAEMON".into()));
                    } else {
                        addr.do_send(WSClientCommand("STOP".into()));
                    }
                    actix_web::HttpResponse::Ok().finish()
                }
                None => actix_web::HttpResponse::NotFound().finish(),
            }
        } else {
            actix_web::HttpResponse::InternalServerError().finish()
        }
    }


    pub fn run(&self, wdid: &str, bindaddr: &str) -> Result<(), Box<dyn std::error::Error>> {
        if self.cached_api_token.is_none() {
            return error("No valid API tokens found.");
        }

        // Try to start via daemon, if exists
        let url = format!("http://{}/start/{}", bindaddr, wdid);
        let mut sys = System::new("dclient");
        let res = actix::SystemRunner::block_on(&mut sys, async {
            awc::Client::new().post(url).send().await
        });
        match res {
            Ok(res) => {
                if res.status() == 403 {
                    warn!("ignoring because daemon already advertising {}", wdid);
                } else {
                    info!("started via existing daemon");
                }
                return Ok(());
            }
            Err(err) => warn!("no existing daemon: {}", err),
        };

        // Else, start new daemon
        info!("starting new daemon");
        let bindaddr: std::net::SocketAddr = bindaddr.parse()?;
        let wdid = String::from(wdid);

        let sys = System::new("wsclient");
        let (err_notify, err_rx) = mpsc::channel();
        let ac = Arc::new(Mutex::new(self.clone()));
        Arbiter::spawn(async move {
            let mut ac_inner = ac.lock().unwrap();
            let addr = match HSAPIClient::ad(&*ac_inner, wdid.clone()).await {
                Ok(a) => a,
                Err(err) => {
                    err_notify.send(format!("{}", err)).unwrap();
                    System::current().stop_with_code(1);
                    return;
                }
            };
            let mut wdid_tab = HashMap::new();
            wdid_tab.insert(wdid.clone(), addr.clone());
            ac_inner.wdid_tab = Some(wdid_tab);
            drop(ac_inner);

            let mut manip = actix_web::HttpServer::new(move || {
                let mut ac = Arc::clone(&ac);
                actix_web::App::new()
                    .data(ac)
                    .wrap(actix_web::middleware::Logger::default())
                    .route(
                        "/stop/{wdid:.*}",
                        actix_web::web::post().to(HSAPIClient::http_post_stop),
                    )
                    .route(
                        "/start/{wdid:.*}",
                        actix_web::web::post().to(HSAPIClient::http_post_start),
                    )
            })
            .workers(1);
            manip = match manip.bind(bindaddr) {
                Ok(s) => s,
                Err(err) => {
                    err_notify
                        .send(format!("failed to bind to {}; {}", bindaddr, err))
                        .unwrap();
                    System::current().stop_with_code(1);
                    return;
                }
            };
            match manip.run().await {
                Ok(()) => (),
                Err(err) => {
                    err_notify
                        .send(format!("failed to start listener: {}", err))
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


    pub fn register_new(&mut self, at_most_1: bool) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(local_config) = &self.local_config {
            if at_most_1 && !local_config.wdeployments.is_empty() {
                return error("local configuration already declares a workspace deployment (to register more, `hardshare register --permit-more`)");
            }
        } else {
            return error("cannot register without initial local configuration. (try `hardshare config --create`)");
        }

        let url = format!("{}/hardshare/register", self.origin);
        let connector = SslConnector::builder(SslMethod::tls())?.build();
        let authheader = format!("Bearer {}", self.cached_api_token.as_ref().unwrap());

        let mut sys = System::new("wclient");
        let res = actix::SystemRunner::block_on(&mut sys, async {
            let client = awc::Client::builder()
                .connector(awc::Connector::new().ssl(connector).finish())
                .header("Authorization", authheader)
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
        let mut new_wd = res?;

        if !new_wd.contains_key("cprovider") {
            new_wd.insert("cprovider".into(), json!("docker"));
        }
        if !new_wd.contains_key("cargs") {
            new_wd.insert("cargs".into(), json!([]));
        }
        if !new_wd.contains_key("image") {
            new_wd.insert("image".into(), json!("rerobots/hs-generic"));
        }
        if !new_wd.contains_key("terminate") {
            new_wd.insert("terminate".into(), json!([]));
        }
        if !new_wd.contains_key("init_inside") {
            new_wd.insert("init_inside".into(), json!([]));
        }
        if !new_wd.contains_key("container_name") {
            new_wd.insert("container_name".into(), json!("rrc"));
        }

        let wdid = String::from(new_wd["id"].as_str().unwrap());
        if let Some(local_config) = &mut self.local_config {
            local_config.wdeployments.push(new_wd);

            #[cfg(not(test))]
            mgmt::modify_local(local_config)?;
        }
        Ok(wdid)
    }

    pub fn declare_existing(&mut self, wdid: &str) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(local_config) = &self.local_config {
            for wd in local_config.wdeployments.iter() {
                if wd["id"] == wdid {
                    return error("attempted to declare workspace deployment that is already declared in local configuration");
                }
            }
        } else {
            return error("cannot declare existing without initial local configuration. (try `hardshare config --create`)");
        }

        let api_token = self.cached_api_token.clone();
        let origin = self.origin.clone();
        let url = format!("{}/hardshare/list", origin);
        let mut sys = System::new("wclient");

        let wdid = wdid.to_string();
        let res = actix::SystemRunner::block_on(&mut sys, async move {
            let client = create_client(api_token)?;

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
                            json!(parsed_body["owner"].as_str().unwrap())
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

        let mut matched_wd = match res? {
            Some(matched_wd) => matched_wd,
            None => {
                return error("no previously registered workspace deployments found with given ID")
            }
        };
        if !matched_wd.contains_key("cprovider") {
            matched_wd.insert("cprovider".into(), json!("docker"));
        }
        if !matched_wd.contains_key("cargs") {
            matched_wd.insert("cargs".into(), json!([]));
        }
        if !matched_wd.contains_key("image") {
            matched_wd.insert("image".into(), json!("rerobots/hs-generic"));
        }
        if !matched_wd.contains_key("terminate") {
            matched_wd.insert("terminate".into(), json!([]));
        }
        if !matched_wd.contains_key("init_inside") {
            matched_wd.insert("init_inside".into(), json!([]));
        }
        if !matched_wd.contains_key("container_name") {
            matched_wd.insert("container_name".into(), json!("rrc"));
        }

        let wdid = String::from(matched_wd["id"].as_str().unwrap());
        if let Some(local_config) = &mut self.local_config {
            local_config.wdeployments.push(matched_wd);

            #[cfg(not(test))]
            mgmt::modify_local(local_config)?;
        }
        Ok(())
    }
}


#[derive(PartialEq, Debug, Clone)]
enum ConnType {
    SshTun,
}

#[derive(PartialEq, Debug, Clone)]
enum CWorkerCommandType {
    InstanceLaunch,
    InstanceDestroy,
    InstanceStatus,
    CreateSshTunDone,
    HubPing,
}

#[derive(Debug, Clone)]
struct CWorkerCommand {
    command: CWorkerCommandType,
    instance_id: String, // \in UUID
    conntype: Option<ConnType>,
    publickey: Option<String>,
    message_id: Option<String>,
}


#[derive(Debug)]
enum InstanceStatus {
    Init,
    InitFail,
    Ready,
    Terminating,
}

impl ToString for InstanceStatus {
    fn to_string(&self) -> String {
        match self {
            InstanceStatus::Init => "INIT".into(),
            InstanceStatus::InitFail => "INIT_FAIL".into(),
            InstanceStatus::Ready => "READY".into(),
            InstanceStatus::Terminating => "TERMINATING".into(),
        }
    }
}

#[derive(Debug)]
struct CurrentInstance {
    status: Option<InstanceStatus>,
    id: Option<String>,
}

impl CurrentInstance {
    pub fn new() -> CurrentInstance {
        CurrentInstance {
            status: None,
            id: None,
        }
    }

    pub fn init(&mut self, instance_id: &str) -> Result<(), &str> {
        if self.exists() {
            return Err("already current instance, cannot INIT new instance");
        }
        self.status = Some(InstanceStatus::Init);
        self.id = Some(instance_id.into());
        Ok(())
    }

    pub fn exists(&self) -> bool {
        self.status.is_some()
    }
}


fn cworker(
    ac: HSAPIClient,
    wsclient_req: mpsc::Receiver<CWorkerCommand>,
    wsclient_addr: Addr<WSClient>,
) {
    let mut current_instance = CurrentInstance::new();

    loop {
        let req = match wsclient_req.recv() {
            Ok(m) => m,
            Err(_) => return,
        };
        debug!("cworker rx: {:?}", req);

        match req.command {
            CWorkerCommandType::InstanceLaunch => {
                match current_instance.init(&req.instance_id) {
                    Ok(()) => {
                        wsclient_addr.do_send(WSClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "ACK",
                                    "mi": req.message_id,
                                }))
                                .unwrap()
                            ),
                        });
                    }
                    Err(err) => {
                        error!(
                            "launch request for instance {} failed: {}",
                            req.instance_id, err
                        );
                        wsclient_addr.do_send(WSClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "NACK",
                                    "mi": req.message_id,
                                }))
                                .unwrap()
                            ),
                        });
                    }
                };

            }
            CWorkerCommandType::InstanceDestroy => {
                if current_instance.exists() {
                    wsclient_addr.do_send(WSClientWorkerMessage {
                        mtype: CWorkerMessageType::WsSend,
                        body: Some(
                            serde_json::to_string(&json!({
                                "v": 0,
                                "cmd": "ACK",
                                "mi": req.message_id,
                            }))
                            .unwrap()
                        ),
                    });
                } else {
                    error!("destroy request received when there is no active instance");
                    wsclient_addr.do_send(WSClientWorkerMessage {
                        mtype: CWorkerMessageType::WsSend,
                        body: Some(
                            serde_json::to_string(&json!({
                                "v": 0,
                                "cmd": "NACK",
                                "mi": req.message_id,
                            }))
                            .unwrap()
                        ),
                    });
                }
            }
            CWorkerCommandType::InstanceStatus => {
                match &current_instance.status {
                    Some(status) => {
                        wsclient_addr.do_send(WSClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "ACK",
                                    "s": status.to_string(),
                                    "mi": req.message_id,
                                }))
                                .unwrap()
                            ),
                        });
                    }
                    None => {
                        warn!("status check received when there is no active instance");
                        wsclient_addr.do_send(WSClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "NACK",
                                    "mi": req.message_id,
                                }))
                                .unwrap()
                            ),
                        });
                    }
                };
            }
            CWorkerCommandType::CreateSshTunDone => {
            }
            CWorkerCommandType::HubPing => {
            }
        }
    }
}


struct WSClient {
    worker_req: mpsc::Sender<CWorkerCommand>,
    ws_sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
    recent_rx_instant: std::time::Instant,
}

#[derive(Message)]
#[rtype(result = "()")]
struct WSClientCommand(String);

#[derive(PartialEq, Debug, Clone)]
enum CWorkerMessageType {
    WsSend,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
struct WSClientWorkerMessage {
    mtype: CWorkerMessageType,
    body: Option<String>,
}

impl Actor for WSClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.check_receive_timeout(ctx);
    }


    fn stopped(&mut self, ctx: &mut Context<Self>) {
        debug!("WSClient actor stopped");
    }
}

impl WSClient {
    fn check_receive_timeout(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(60, 0), |act, ctx| {
            if act.recent_rx_instant.elapsed() > Duration::new(45, 0) {
                debug!("timeout waiting for server");
                act.ws_sink.write(Message::Close(None));
                ctx.stop();
            } else {
                act.check_receive_timeout(ctx);
            }
        });
    }
}

impl Handler<WSClientCommand> for WSClient {
    type Result = ();

    fn handle(&mut self, msg: WSClientCommand, ctx: &mut Context<Self>) {
        debug!("received client command: {}", msg.0);
        if msg.0 == "STOP" {
            ctx.stop();
        } else if msg.0 == "STOP DAEMON" {
            ctx.stop();
            System::current().stop();
        } else {
            warn!("unknown client command: {}", msg.0);
        }
    }
}

impl Handler<WSClientWorkerMessage> for WSClient {
    type Result = ();

    fn handle(&mut self, msg: WSClientWorkerMessage, ctx: &mut Context<Self>) {
        debug!("received client worker message: {:?}", msg);
        match msg.mtype {
            CWorkerMessageType::WsSend => {
                self.ws_sink.write(Message::Text(msg.body.unwrap()));
            }
        }
    }
}

impl StreamHandler<Result<Frame, WsProtocolError>> for WSClient {
    fn handle(&mut self, msg: Result<Frame, WsProtocolError>, ctx: &mut Context<Self>) {
        self.recent_rx_instant = std::time::Instant::now();

        if let Ok(Frame::Text(txt)) = msg {
            let payload: serde_json::Value = match serde_json::from_slice(txt.as_ref()) {
                Ok(p) => p,
                Err(err) => {
                    error!("failed to parse {:?}: {}", txt, err);
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
                error!(
                    "received message of unknown format version: {}",
                    message_ver
                );
                return;
            }

            let cmd = match payload["cmd"].as_str() {
                Some(c) => c,
                None => {
                    error!("received message of without `cmd` field");
                    return;
                }
            };

            let m = match cmd {
                "INSTANCE_LAUNCH" => CWorkerCommand {
                    command: CWorkerCommandType::InstanceLaunch,
                    instance_id: String::from(payload["id"].as_str().unwrap()),
                    conntype: Some(ConnType::SshTun), // TODO: Support ct != sshtun
                    publickey: Some(String::from(payload["pr"].as_str().unwrap())),
                    message_id: Some(String::from(payload["mi"].as_str().unwrap()))
                },
                "INSTANCE_STATUS" => CWorkerCommand {
                    command: CWorkerCommandType::InstanceStatus,
                    instance_id: String::from(payload["id"].as_str().unwrap()),
                    conntype: None,
                    publickey: None,
                    message_id: Some(String::from(payload["mi"].as_str().unwrap())),
                },
                "INSTANCE_DESTROY" => CWorkerCommand {
                    command: CWorkerCommandType::InstanceDestroy,
                    instance_id: String::from(payload["id"].as_str().unwrap()),
                    conntype: None,
                    publickey: None,
                    message_id: Some(String::from(payload["mi"].as_str().unwrap())),
                },
                _ => {
                    error!("unknown command: {}", cmd);
                    return;
                }
            };
            self.worker_req.send(m).unwrap();
        } else if let Ok(Frame::Ping(_)) = msg {
            debug!("received PING; sending PONG");
            self.ws_sink.write(Message::Pong(Bytes::from_static(b"")));
        } else {
            debug!("unrecognized WebSocket message: {:?}", msg);
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        debug!("StreamHandler of WSClient is finished");
        ctx.stop()
    }
}

impl actix::io::WriteHandler<WsProtocolError> for WSClient {}


#[cfg(test)]
mod tests {
    use mockito::mock;

    use super::mgmt;
    use super::AddOn;
    use super::HSAPIClient;
    use super::CurrentInstance;

    #[test]
    fn list_no_rules() {
        let wdid = "68a1be97-9365-4007-b726-14c56bd69eef";
        let path = format!("/deployment/{}/rules", wdid);
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
        let path = format!("/deployment/{}", wdid);
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
        let expected_new_wdids = vec![
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

    #[test]
    fn cannot_init_when_busy() {
        let instance_ids = vec![
            "e5fcf112-7af2-4d9f-93ce-b93f0da9144d",
            "0f2576b5-17d9-477e-ba70-f07142faa2d9",
        ];
        let mut current_instance = CurrentInstance::new();
        assert!(current_instance.init(instance_ids[0]).is_ok());
        assert!(current_instance.exists());
        assert!(current_instance.init(instance_ids[1]).is_err());
    }
}
