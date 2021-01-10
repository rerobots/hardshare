// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

extern crate reqwest;

extern crate serde_json;
extern crate serde;
use serde::{Serialize, Deserialize};

use crate::mgmt;


#[derive(Debug)]
pub struct HSAPIClient {
    local_config: Option<mgmt::Config>,
    default_key_index: Option<u16>,
    cached_key: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RemoteConfig {

}

impl HSAPIClient {
    pub fn new() -> HSAPIClient {
        match mgmt::get_local_config(false, false) {
            Ok(local_config) => HSAPIClient {
                local_config: Some(local_config),
                default_key_index: None,
                cached_key: None,
            },
            Err(_) => HSAPIClient {
                local_config: None,
                default_key_index: None,
                cached_key: None,
            }
        }
    }


    pub fn get_remote_config(&self, include_dissolved: bool) {

    }
}
