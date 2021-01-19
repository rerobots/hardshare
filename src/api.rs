// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::collections::HashMap;
use std::time::Duration;

use actix::io::SinkWrite;
use actix::*;
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
            return error("No valid API tokens found.  Try\n\n    hardshare config -l --local");
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
        let authheader = format!("Bearer {}", self.cached_key.as_ref().unwrap());
        let bindaddr: std::net::SocketAddr = bindaddr.parse()?;

        let sys = System::new("wsclient");
        Arbiter::spawn(async move {

            let client = awc::Client::builder()
                .connector(awc::Connector::new().ssl(connector).finish())
                .header("Authorization", authheader)
                .finish();

            let (_, framed) = client.ws(url).connect().await.unwrap();
            let (sink, stream) = framed.split();
            let addr = WSClient::create(|ctx| {
                WSClient::add_stream(stream, ctx);
                WSClient(SinkWrite::new(sink, ctx))
            });

            actix_web::HttpServer::new(move || {
                let addr = addr.clone();
                actix_web::App::new().route("/stop", actix_web::web::post().to(move || {
                    addr.do_send(WSClientCommand("STOP".into()));
                    actix_web::HttpResponse::Ok()
                }))
            })
                .workers(1)
                .bind(bindaddr).unwrap()
                .run()
                .await.unwrap()

        });
        sys.run()?;

        Ok(())
    }
}


struct WSClient(SinkWrite<Message, SplitSink<Framed<BoxedSocket, Codec>, Message>>);

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
            act.0.write(Message::Ping(Bytes::from_static(b"")));
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
        }
    }

    fn finished(&mut self, ctx: &mut Context<Self>) {
        ctx.stop()
    }
}

impl actix::io::WriteHandler<WsProtocolError> for WSClient {}
