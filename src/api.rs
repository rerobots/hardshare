// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

extern crate reqwest;

extern crate serde_json;
extern crate serde;
use serde::{Serialize, Deserialize};

extern crate tokio;
use tokio::runtime::Runtime;

use crate::mgmt;


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


    pub fn get_remote_config(&self, include_dissolved: bool) -> Result<serde_json::Value, String> {
        if self.cached_key.is_none() {
            return Err("No valid API tokens found.  Try\n\n    hardshare config -l --local".into());
        }
        let mut rt = Runtime::new().unwrap();
        rt.block_on(async {

            let hslisturl = if include_dissolved {
                "/list?with_dissolved"
            } else {
                "/list"
            };

            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(reqwest::header::AUTHORIZATION,
                           format!("Bearer {}", self.cached_key.as_ref().unwrap()).parse().unwrap());
            let client = reqwest::Client::builder()
                .default_headers(headers)
                .build().unwrap();

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
}
