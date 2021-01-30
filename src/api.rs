// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::collections::HashMap;
use std::sync::mpsc;
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

use openssl::ssl::{SslMethod, SslConnector};

extern crate reqwest;

extern crate serde_json;
extern crate serde;
use serde::{Serialize, Deserialize};

extern crate tokio;
use tokio::runtime::Runtime;

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
    S: ToString
{
    Err(Box::new(ClientError { msg: msg.to_string() }))
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
    pub comment: Option<String>
}

impl std::fmt::Display for AccessRules {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_yaml::to_string(self).unwrap())
    }
}


#[derive(Debug)]
pub struct HSAPIClient {
    local_config: Option<mgmt::Config>,
    default_key_index: Option<usize>,
    cached_key: Option<String>,
    origin: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemoteConfig {

}

impl HSAPIClient {
    pub fn new() -> HSAPIClient {
        let mut hsclient = match mgmt::get_local_config(false, false) {
            Ok(local_config) => HSAPIClient {
                local_config: Some(local_config),
                default_key_index: None,
                cached_key: None,
                origin: String::from("https://hs.rerobots.net"),
            },
            Err(_) => return HSAPIClient {
                local_config: None,
                default_key_index: None,
                cached_key: None,
                origin: String::from("https://hs.rerobots.net"),
            }
        };

        if let Some(local_config) = &hsclient.local_config {
            if local_config.keys.len() > 0 {
                hsclient.default_key_index = Some(0);
                let raw_tok = std::fs::read(&local_config.keys[hsclient.default_key_index.unwrap()]).unwrap();
                let tok = String::from_utf8(raw_tok).unwrap();
                hsclient.cached_key = Some(tok);
            }
        }

        hsclient
    }


    fn url(&self, path: &str) -> reqwest::Url {
        reqwest::Url::parse((self.origin.clone() + path).as_str()).unwrap()
    }


    fn create_authclient(&self) -> Result<reqwest::Client, Box<dyn std::error::Error>> {
        if self.cached_key.is_none() {
            return error("No valid API tokens found.");
        }

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(reqwest::header::AUTHORIZATION,
                       format!("Bearer {}", self.cached_key.as_ref().unwrap()).parse().unwrap());
        Ok(reqwest::Client::builder()
           .default_headers(headers)
           .build().unwrap())
    }


    pub fn get_remote_config(&self, include_dissolved: bool) -> Result<serde_json::Value, String> {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {

            let hslisturl = if include_dissolved {
                "/list?with_dissolved"
            } else {
                "/list"
            };

            let client = match self.create_authclient() {
                Ok(c) => c,
                Err(err) => return Err(format!("{}", err))
            };

            let res = match client.get(self.url(hslisturl)).send().await {
                Ok(r) => r,
                Err(err) => return Err(format!("{}", err))
            };

            let mut payload: serde_json::Value;
            if res.status() == 200 {
                payload = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
            } else if res.status() == 400 {
                let payload: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
                return Err(String::from(payload["error_message"].as_str().unwrap()));
            } else {
                return Err(format!("error contacting hardshare server: {}", res.status()));
            }

            let apilisturl = if include_dissolved {
                "https://api.rerobots.net/hardshare/list?with_dissolved"
            } else {
                "https://api.rerobots.net/hardshare/list"
            };

            let res = match client.get(apilisturl).send().await {
                Ok(r) => r,
                Err(err) => return Err(format!("{}", err))
            };

            if res.status() == 200 {
                let apipayload: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
                for wd in payload["deployments"].as_array_mut().unwrap().iter_mut() {
                    wd["desc"] = apipayload["attr"][wd["id"].as_str().unwrap()]["desc"].clone();
                }

            } else if res.status() == 400 {
                let payload: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
                return Err(String::from(payload["error_message"].as_str().unwrap()));
            } else {
                return Err(format!("error contacting core API server: {}", res.status()));
            }

            Ok(payload)
        })
    }


    async fn get_access_rules_a(&self, client: &reqwest::Client, wdid: &str) -> Result<AccessRules, Box<dyn std::error::Error>> {
        let url = reqwest::Url::parse(format!("https://api.rerobots.net/deployment/{}/rules", wdid).as_str()).unwrap();
        let res = client.get(url).send().await?;
        if res.status() == 200 {

            let payload: AccessRules = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
            Ok(payload)

        } else if res.status() == 400 {
            let payload: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
            error(payload["error_message"].as_str().unwrap())
        } else {
            error(format!("error contacting core API server: {}", res.status()))
        }
    }


    pub fn get_access_rules(&self, wdid: &str) -> Result<AccessRules, Box<dyn std::error::Error>> {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {

            let client = self.create_authclient()?;
            self.get_access_rules_a(&client, wdid).await

        })
    }


    pub fn drop_access_rules(&self, wdid: &str) -> Result<(), Box<dyn std::error::Error>> {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {

            let client = self.create_authclient()?;

            let ruleset = self.get_access_rules_a(&client, wdid).await?;
            for rule in ruleset.rules.iter() {

                let url = reqwest::Url::parse(format!("https://api.rerobots.net/deployment/{}/rule/{}", wdid, rule.id).as_str()).unwrap();
                let res = client.delete(url).send().await?;
                if res.status() != 200 {
                    return error(format!("error deleting rule {}: {}", rule.id, res.status()))
                }

            }

            Ok(())

        })
    }


    pub fn add_access_rule(&self, wdid: &str, to_user: &str) -> Result<(), Box<dyn std::error::Error>> {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {

            let mut body = HashMap::new();
            body.insert("cap", "CAP_INSTANTIATE");
            body.insert("user", to_user);

            let client = self.create_authclient()?;

            let url = reqwest::Url::parse(format!("https://api.rerobots.net/deployment/{}/rule", wdid).as_str()).unwrap();
            let res = client.post(url).json(&body).send().await?;
            if res.status() == 400 {
                let payload: serde_json::Value = serde_json::from_slice(&res.bytes().await.unwrap()).unwrap();
                return error(payload["error_message"].as_str().unwrap())
            } else if res.status() == 404 {
                return error(format!("not found"))
            } else if res.status() != 200 {
                return error(format!("server indicated error: {}", res.status()))
            }

            Ok(())

        })
    }


    pub fn stop(&self, bindaddr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let url = format!("http://{}/stop", bindaddr);
        let mut sys = System::new("dclient");
        actix::SystemRunner::block_on(&mut sys, async {
            awc::Client::new().post(url).send().await.and_then(|resp| {
                Ok(())
            }).or_else(|err| {
                error(err)
            })
        })
    }


    pub fn run(&self, wdid: &str, bindaddr: &str) -> Result<(), Box<dyn std::error::Error>> {

        let url = format!("{}/ad/{}", self.origin, wdid);
        let connector = SslConnector::builder(SslMethod::tls())?.build();
        if self.cached_key.is_none() {
            return error("No valid API tokens found.");
        }
        let authheader = format!("Bearer {}", self.cached_key.as_ref().unwrap());
        let bindaddr: std::net::SocketAddr = bindaddr.parse()?;

        let sys = System::new("wsclient");
        let (err_notify, err_rx) = mpsc::channel();
        Arbiter::spawn(async move {

            let client = awc::Client::builder()
                .connector(awc::Connector::new().ssl(connector).finish())
                .header("Authorization", authheader)
                .finish();

            let (_, framed) = match client.ws(url).connect().await {
                Ok(r) => r,
                Err(err) => {
                    err_notify.send(format!("{}", err)).unwrap();
                    System::current().stop_with_code(1);
                    return
                }
            };
            let (sink, stream) = framed.split();

            let addr = WSClient::create(|ctx| {
                WSClient::add_stream(stream, ctx);
                WSClient {
                    ws_sink: SinkWrite::new(sink, ctx)
                }
            });

            let mut manip = actix_web::HttpServer::new(move || {
                let addr = addr.clone();
                actix_web::App::new().route("/stop", actix_web::web::post().to(move || {
                    addr.do_send(WSClientCommand("STOP".into()));
                    actix_web::HttpResponse::Ok()
                }))
            }).workers(1);
            manip = match manip.bind(bindaddr) {
                Ok(s) => s,
                Err(err) => {
                    err_notify.send(format!("failed to bind to {}; {}", bindaddr, err)).unwrap();
                    System::current().stop_with_code(1);
                    return
                }
            };
            match manip.run().await {
                Ok(()) => (),
                Err(err) => {
                    err_notify.send(format!("failed to start listener: {}", err)).unwrap();
                    System::current().stop_with_code(1);
                    return
                }
            }

        });
        let res = Arbiter::current().join();
        match sys.run() {
            Ok(()) => Ok(()),
            Err(_) => error(err_rx.recv()?)
        }
    }


    pub fn register_new(&mut self, at_most_1: bool) -> Result<String, Box<dyn std::error::Error>> {
        if let Some(local_config) = &self.local_config {
            if at_most_1 && local_config.wdeployments.len() > 0 {
                return error("local configuration already declares a workspace deployment (to register more, `hardshare register --permit-more`)");
            }
        } else {
            return error("cannot register without initial local configuration. (try `hardshare config --create`)");
        }

        let url = format!("{}/register", self.origin);
        let connector = SslConnector::builder(SslMethod::tls())?.build();
        let authheader = format!("Bearer {}", self.cached_key.as_ref().unwrap());

        let mut sys = System::new("wclient");
        let res = actix::SystemRunner::block_on(&mut sys, async {
            let client = awc::Client::builder()
                .connector(awc::Connector::new().ssl(connector).finish())
                .header("Authorization", authheader)
                .finish();
            let mut resp = client.post(url).send().await?;
            if resp.status() == 200 {
                let payload: serde_json::Value = serde_json::from_slice(resp.body().await?.as_ref())?;
                let mut new_wd = HashMap::new();
                new_wd.insert("id".into(), json!(payload["id"].as_str().unwrap()));
                new_wd.insert("owner".into(), json!(payload["owner"].as_str().unwrap()));
                Ok(new_wd)
            } else if resp.status() == 400 {
                let payload: serde_json::Value = serde_json::from_slice(resp.body().await?.as_ref())?;
                error(String::from(payload["error_message"].as_str().unwrap()))
            } else {
                error(format!("server indicated error: {}", resp.status()))
            }
        });
        if res.is_err() {
            return Err(res.unwrap_err());
        }
        let mut new_wd = res.unwrap();

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
            mgmt::modify_local(&local_config)?;
        }
        Ok(wdid)
    }
}


struct WSClient {
    ws_sink: SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>,
}

#[derive(Message)]
#[rtype(result = "()")]
struct WSClientCommand(String);

impl Actor for WSClient {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        self.heartbeat(ctx);
    }


    fn stopped(&mut self, ctx: &mut Context<Self>) {
        System::current().stop();
    }
}

impl WSClient {
    fn heartbeat(&self, ctx: &mut Context<Self>) {
        ctx.run_later(Duration::new(15, 0), |act, ctx| {
            act.ws_sink.write(Message::Ping(Bytes::from_static(b"")));
            act.heartbeat(ctx);

        });
    }
}

impl Handler<WSClientCommand> for WSClient {
    type Result = ();

    fn handle(&mut self, msg: WSClientCommand, ctx: &mut Context<Self>) {
        debug!("received client command: {}", msg.0);
        if msg.0 == "STOP" {
            ctx.stop();
        } else {
            warn!("unknown client command: {}", msg.0);
        }
    }
}

impl StreamHandler<Result<Frame, WsProtocolError>> for WSClient {
    fn handle(&mut self, msg: Result<Frame, WsProtocolError>, ctx: &mut Context<Self>) {
        if let Ok(Frame::Text(txt)) = msg {
            let payload: serde_json::Value = match serde_json::from_slice(txt.as_ref()) {
                Ok(p) => p,
                Err(err) => {
                    error!("failed to parse {:?}: {}", txt, err);
                    return;
                }
            };
            debug!("received: {}", serde_json::to_string(&payload).unwrap());
            let cmd = payload["cmd"].as_str().unwrap();
            if cmd == "INSTANCE_LAUNCH" {
            }
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        ctx.stop()
    }
}

impl actix::io::WriteHandler<WsProtocolError> for WSClient {}
