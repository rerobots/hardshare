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

use assert_cmd::Command;
use tempfile::{tempdir, NamedTempFile};

#[test]
fn prints_version() {
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("-V").assert();
    assert
        .stdout(format!("{}\n", env!("CARGO_PKG_VERSION")))
        .success();
}

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("help").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap());

    // Alternative style: -h
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("-h").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!("prints_help", String::from_utf8(output.stdout).unwrap());

    // Alternative style: --help
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("--help").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!("prints_help", String::from_utf8(output.stdout).unwrap());
}

#[test]
fn prints_help_config() {
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("help").arg("config").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!(String::from_utf8(output.stdout).unwrap());

    // Alternative style: -h
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("config").arg("-h").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!(
        "prints_help_config",
        String::from_utf8(output.stdout).unwrap()
    );

    // Alternative style: --help
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("config").arg("--help").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!(
        "prints_help_config",
        String::from_utf8(output.stdout).unwrap()
    );
}

#[test]
fn prints_help_register() {
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("help").arg("register").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!(
        "prints_help_register",
        String::from_utf8(output.stdout).unwrap()
    );

    // Alternative style: -h
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("register").arg("-h").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!(
        "prints_help_register",
        String::from_utf8(output.stdout).unwrap()
    );

    // Alternative style: --help
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd.arg("register").arg("--help").assert();
    let output = assert.get_output().clone();
    assert.success();
    insta::assert_snapshot!(
        "prints_help_register",
        String::from_utf8(output.stdout).unwrap()
    );
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
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd
        .arg("config")
        .arg("--add-key")
        .arg(ntf.path().join("notexist"))
        .assert();
    assert.failure().code(1);
}

#[test]
fn list_config_does_not_exist() {
    let tmphome = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("hardshare").unwrap();
    let assert = cmd
        .env("HOME", tmphome.path())
        .arg("config")
        .arg("--local")
        .arg("-l")
        .assert();
    assert.failure().code(1);
}
