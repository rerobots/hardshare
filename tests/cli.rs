// SCL <scott@rerobots.net>
// Copyright (C) 2020 rerobots, Inc.

use std::io::prelude::*;

use assert_cmd::Command;
use tempfile::{NamedTempFile, tempdir};


#[test]
fn prints_version() {
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("-V").assert();
    assert.stdout(format!("{}\n", env!("CARGO_PKG_VERSION"))).success();
}


#[test]
fn config_requires_arg() {
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("config").assert();
    assert.failure().code(1);
}


#[test]
fn add_token_does_not_exist() {
    let ntf = NamedTempFile::new().unwrap();
    println!("{:?}", ntf.path().join("notexist"));
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("config").arg("--add-key").arg(ntf.path().join("notexist")).assert();
    assert.failure().code(1);
}


#[test]
fn list_config_does_not_exist() {
    let tmphome = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.env("HOME", tmphome.path()).arg("config").arg("--local").arg("-l").assert();
    assert.failure().code(1);
}
