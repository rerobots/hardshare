// Copyright (C) 2023 rerobots, Inc.
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
use std::fs::File;
use std::io::prelude::*;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::sync::atomic::{self, AtomicBool};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use actix::prelude::*;
use log::{log_enabled, Level};
use serde::Deserialize;
use tempfile::NamedTempFile;

use crate::api;
use crate::check::Error;
use crate::mgmt::{CProvider, WDeployment};

#[derive(PartialEq, Debug, Clone)]
enum InstanceStatus {
    Init,
    InitFail,
    Ready,
    Terminating,
    Fault,
}

impl std::fmt::Display for InstanceStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstanceStatus::Init => write!(f, "INIT"),
            InstanceStatus::InitFail => write!(f, "INIT_FAIL"),
            InstanceStatus::Ready => write!(f, "READY"),
            InstanceStatus::Terminating => write!(f, "TERMINATING"),
            InstanceStatus::Fault => write!(f, "FAULT"),
        }
    }
}

type Port = u32;

#[derive(PartialEq, Debug, Clone)]
pub enum ConnType {
    SshTun,
}

pub struct ContainerAddress {
    ip: String,
    port: Port,
    hostkey: String,
    subprocess: Option<std::process::Child>,
}

struct SshTunnel {
    proc: std::process::Child,
    container_addr: ContainerAddress,
}

#[derive(Clone)]
pub struct CurrentInstance {
    wdeployment: Arc<WDeployment>,
    status: Arc<Mutex<Option<InstanceStatus>>>,
    id: Option<String>,
    local_name: Arc<Mutex<Option<String>>>,
    main_actor_addr: Option<Addr<api::MainActor>>,
    responses: Arc<Mutex<HashMap<String, Option<CWorkerCommand>>>>,
    tunnel: Arc<Mutex<Option<SshTunnel>>>,
}

impl CurrentInstance {
    fn new(
        wdeployment: &Arc<WDeployment>,
        main_actor_addr: Option<&Addr<api::MainActor>>,
    ) -> CurrentInstance {
        CurrentInstance {
            wdeployment: Arc::clone(wdeployment),
            status: Arc::new(Mutex::new(None)),
            id: None,
            local_name: Arc::new(Mutex::new(None)),
            main_actor_addr: main_actor_addr.cloned(),
            responses: Arc::new(Mutex::new(HashMap::new())),
            tunnel: Arc::new(Mutex::new(None)),
        }
    }

    fn generate_local_name(&mut self, base_name: &str) -> String {
        let random_suffix: String = rand::random::<u16>().to_string();
        let mut local_name = self
            .local_name
            .lock()
            .expect("Local container name lock can be held");
        let name = base_name.to_string() + &random_suffix;
        *local_name = Some(name.clone());
        name
    }

    fn get_local_name(&self) -> Result<String, &str> {
        let local_name = self
            .local_name
            .lock()
            .expect("Local container name lock can be held");
        local_name
            .clone()
            .ok_or("Local container name is undefined")
    }

    fn handle_response(&mut self, res: &CWorkerCommand) -> Result<(), String> {
        let message_id: String = match res.message_id.clone() {
            Some(mi) => mi,
            None => {
                return Err("missing message_id".to_string());
            }
        };
        let mut responses = self
            .responses
            .lock()
            .expect("Lock on responses table can be held");
        if !responses.contains_key(&message_id) {
            return Err(format!("unknown message {message_id}"));
        }
        if responses[&message_id].is_some() {
            return Err(format!("already handled message {message_id}"));
        }
        responses.insert(message_id, Some(res.clone()));
        Ok(())
    }

    fn send_status(&self) {
        if let Some(main_actor_addr) = &self.main_actor_addr {
            let status = self
                .status
                .lock()
                .expect("Instance status lock can be held");
            match &*status {
                Some(s) => {
                    let mut msg = json!({
                    "v": 0,
                    "cmd": "INSTANCE_STATUS",
                    "s": s.to_string(),
                    });

                    if *s != InstanceStatus::Ready && *s != InstanceStatus::Init {
                        main_actor_addr.do_send(api::ClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&msg)
                                    .expect("Instance status message can be serialized to JSON"),
                            ),
                        });
                    } else {
                        let hostkey = {
                            let tunnel =
                                self.tunnel.lock().expect("Lock on tunnel info can be held");
                            (*tunnel).as_ref().map(|t| t.container_addr.hostkey.clone())
                        };

                        main_actor_addr.do_send(api::ClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&match hostkey {
                                    Some(h) => {
                                        msg["h"] = h.into();
                                        msg
                                    }
                                    None => msg,
                                })
                                .expect("Instance status message can be serialized to JSON"),
                            ),
                        });
                    }
                }
                None => {
                    error!("called when no active instance");
                }
            }
        }
    }

    fn send_create_sshtun(
        &self,
        tunnelkey_public: &str,
        proxy_mode: bool,
    ) -> Result<String, String> {
        if let Some(main_actor_addr) = &self.main_actor_addr {
            let message_id = {
                let mut message_id: String;
                let mut responses = self
                    .responses
                    .lock()
                    .expect("Lock on responses table can be held");
                loop {
                    message_id = rand::random::<u32>().to_string();
                    if !responses.contains_key(&message_id) {
                        responses.insert(message_id.clone(), None);
                        break;
                    }
                }
                message_id
            };
            let status = self
                .status
                .lock()
                .expect("Instance status lock can be held");
            match &*status {
                Some(s) => {
                    if *s != InstanceStatus::Init && *s != InstanceStatus::Ready {
                        Err(format!("called when instance status {s}"))
                    } else {
                        main_actor_addr.do_send(api::ClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "CREATE_SSHTUN",
                                    "id": self.id.as_ref().clone(),
                                    "key": tunnelkey_public,
                                    "mi": message_id,
                                    "proxy": proxy_mode,
                                }))
                                .expect("SSH tunnel creation message can be serialized to JSON"),
                            ),
                        });
                        Ok(message_id)
                    }
                }
                None => Err("called when no active instance".into()),
            }
        } else {
            Err("called without WebSocket client".into())
        }
    }

    fn init(
        &mut self,
        instance_id: &str,
        conn_type: ConnType,
        public_key: &str,
        repo_args: Option<RepoInfo>,
    ) -> Result<(thread::JoinHandle<()>, Arc<AtomicBool>), &str> {
        let mut status = self
            .status
            .lock()
            .expect("Instance status lock can be held");
        match *status {
            Some(_) => {
                return Err("already current instance, cannot INIT new instance");
            }
            None => {
                *status = Some(InstanceStatus::Init);
                self.id = Some(instance_id.into());
            }
        }

        let instance = self.clone();
        let public_key = String::from(public_key);

        let abort_launch = Arc::new(AtomicBool::new(false));
        let abort_launch_clone = abort_launch.clone();

        Ok((
            thread::spawn(move || match conn_type {
                ConnType::SshTun => CurrentInstance::launch_sshtun(
                    instance,
                    &public_key,
                    repo_args,
                    abort_launch_clone,
                ),
            }),
            abort_launch,
        ))
    }

    fn exists(&self) -> bool {
        self.status
            .lock()
            .expect("Instance status lock can be held")
            .is_some()
    }

    fn status(&self) -> Option<InstanceStatus> {
        (*self
            .status
            .lock()
            .expect("Instance status lock can be held"))
        .as_ref()
        .cloned()
    }

    fn declare_status(&mut self, new_status: InstanceStatus) {
        let mut x = self
            .status
            .lock()
            .expect("Instance status lock can be held");
        *x = Some(new_status);
    }

    fn clear_status(&mut self) {
        let mut x = self
            .status
            .lock()
            .expect("Instance status lock can be held");
        if *x != Some(InstanceStatus::Fault) {
            *x = None;
        }
    }

    fn get_container_addr(
        cprovider: &CProvider,
        name: &str,
        timeout: u64,
    ) -> Result<String, String> {
        let execname = cprovider.get_execname().unwrap();
        let max_duration = std::time::Duration::from_secs(timeout);
        let sleep_time = std::time::Duration::from_secs(2);
        let now = std::time::Instant::now();
        while now.elapsed() <= max_duration {
            let mut run_command = Command::new(&execname);
            let run_command = run_command.args(["inspect", name]);
            let command_result = match run_command.output() {
                Ok(o) => o,
                Err(err) => return Err(format!("{err}")),
            };
            if !command_result.status.success() {
                return Err(format!("run command failed: {command_result:?}"));
            }
            let r: serde_json::Value = match serde_json::from_slice(&command_result.stdout) {
                Ok(o) => o,
                Err(err) => return Err(format!("{err}")),
            };
            match r[0]["NetworkSettings"]["IPAddress"].as_str() {
                Some(addr) => {
                    if !addr.is_empty() {
                        return Ok(addr.into());
                    }
                    warn!("waiting for address...");
                    std::thread::sleep(sleep_time);
                }
                None => {
                    warn!("waiting for address...");
                    std::thread::sleep(sleep_time);
                }
            }
        }
        Err("address not found".into())
    }

    fn get_container_sshport(cprovider: &CProvider, name: &str) -> Result<Port, String> {
        let execname = cprovider.get_execname().unwrap();
        let mut run_command = Command::new(execname);
        let run_command = run_command.args(["port", name, "22"]);
        let command_result = match run_command.output() {
            Ok(o) => o,
            Err(err) => return Err(format!("{err}")),
        };
        if !command_result.status.success() {
            return Err(format!("run command failed: {command_result:?}"));
        }

        let s = String::from_utf8(command_result.stdout).expect("valid UTF-8 encoding");
        let s = s.trim();
        let parts: Vec<&str> = s.split(':').collect();
        match Port::from_str(parts[1]) {
            Ok(port) => Ok(port),
            Err(err) => Err(format!("SSH port not found: {err}")),
        }
    }

    fn get_container_hostkey(
        cprovider: &CProvider,
        name: &str,
        timeout: u64,
    ) -> Result<String, String> {
        let execname = cprovider.get_execname().unwrap();
        let hostkey_filename = "ssh_host_ecdsa_key.pub";
        let hostkey_contained_path = String::from(name) + ":/etc/ssh/" + hostkey_filename;
        let max_duration = std::time::Duration::from_secs(timeout);
        let sleep_time = std::time::Duration::from_secs(2);
        let now = std::time::Instant::now();
        while now.elapsed() <= max_duration {
            match Command::new(&execname)
                .args(["cp", &hostkey_contained_path, "."])
                .status()
            {
                Ok(copy_result) => {
                    if copy_result.success() {
                        let mut hostkey_file = match File::open(hostkey_filename) {
                            Ok(f) => f,
                            Err(err) => return Err(format!("{err}")),
                        };
                        let mut hostkey = String::new();
                        if let Err(err) = hostkey_file.read_to_string(&mut hostkey) {
                            return Err(format!("{err}"));
                        }
                        drop(hostkey_file);
                        if let Err(err) = std::fs::remove_file(hostkey_filename) {
                            error!("Failed to remove file {hostkey_filename}; caught: {err}");
                        }
                        return Ok(hostkey);
                    } else {
                        warn!("waiting for host key...");
                        std::thread::sleep(sleep_time);
                    }
                }
                Err(_) => {
                    warn!("waiting for host key...");
                    std::thread::sleep(sleep_time);
                }
            }
        }
        Err("host key not found".into())
    }

    fn start_proxy(
        cargs: &[String],
        timeout: u64,
    ) -> Result<(std::process::Child, Port), Box<dyn std::error::Error>> {
        if cargs[0] != "rrhttp" {
            return Err("only rrhttp proxy supported".into());
        }
        let mut child = Command::new(&cargs[0])
            .args(cargs[1..].iter())
            .stdout(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.as_mut().unwrap();
        let max_duration = std::time::Duration::from_secs(timeout);
        let sleep_time = std::time::Duration::from_secs(1);
        let now = std::time::Instant::now();
        let mut acc: Vec<u8> = vec![];
        while now.elapsed() <= max_duration {
            let mut buf = [0; 32];
            let n = stdout.read(&mut buf)?;
            for b in buf.iter().take(n) {
                if *b == 0x0a {
                    let line = String::from_utf8(acc)?;
                    let parts: Vec<&str> = line.split(':').collect();
                    let port = Port::from_str(parts[1])?;
                    return Ok((child, port));
                }
                acc.push(*b);
            }
            std::thread::sleep(sleep_time);
        }
        Err("port not found".into())
    }

    fn start_sshtun(
        &self,
        container_addr: ContainerAddress,
        tunnelkey_path: &str,
        timeout: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let tunnelkey_public_path = String::from(tunnelkey_path) + ".pub";
        let mut f = File::open(tunnelkey_public_path)?;
        let mut tunnelkey_public = String::new();
        f.read_to_string(&mut tunnelkey_public)?;
        let proxy_mode = self.wdeployment.cprovider == CProvider::Proxy;
        let message_id = self.send_create_sshtun(&tunnelkey_public, proxy_mode)?;
        let st = std::time::Duration::from_secs(2);
        let mut tunnelinfo = None;

        let max_duration = std::time::Duration::from_secs(timeout);
        let now = std::time::Instant::now();
        while now.elapsed() <= max_duration {
            std::thread::sleep(st);
            {
                let responses = self
                    .responses
                    .lock()
                    .expect("Lock on responses table can be held");
                if let Some(res) = &responses[&message_id] {
                    let ti = res.tunnelinfo.clone().unwrap();
                    info!(
                        "opened public ssh tunnel at {}:{} with host key \"{}\"",
                        ti.ipv4, ti.port, ti.hostkey
                    );
                    tunnelinfo = Some(ti);
                    break;
                }
            }
            info!("waiting for sshtun creation...");
        }
        if tunnelinfo.is_none() {
            return Err("failed to create sshtun within time limit".into());
        }
        let tunnelinfo = tunnelinfo.unwrap();

        let tunnel_process_args = [
            "-o",
            "ServerAliveInterval=10",
            "-o",
            "StrictHostKeyChecking=no",
            "-o",
            "ExitOnForwardFailure=yes",
            "-T",
            "-N",
            "-R",
            &format!(":2210:{}:{}", container_addr.ip, container_addr.port),
            "-i",
            tunnelkey_path,
            "-p",
            &format!("{thport}", thport = tunnelinfo.thport),
            &format!(
                "{thuser}@{addr}",
                thuser = tunnelinfo.thuser,
                addr = tunnelinfo.ipv4
            ),
        ];
        info!("tunnel process args: {tunnel_process_args:?}");
        let tunnel_process = Command::new("ssh").args(tunnel_process_args).spawn()?;

        let mut tunnel = self.tunnel.lock().expect("Lock on tunnel info can be held");
        *tunnel = Some(SshTunnel {
            proc: tunnel_process,
            container_addr,
        });
        Ok(())
    }

    fn launch_sshtun(
        mut instance: CurrentInstance,
        public_key: &str,
        repo_args: Option<RepoInfo>,
        abort_launch: Arc<AtomicBool>,
    ) {
        let base_name = instance.wdeployment.container_name.clone();
        let name = instance.generate_local_name(&base_name);
        let container_addr = match Self::launch_container(&instance.wdeployment, &name, public_key)
        {
            Ok(ca) => ca,
            Err(err) => {
                error!("{err}");
                instance.declare_status(InstanceStatus::InitFail);
                instance.send_status();
                return;
            }
        };
        if abort_launch.load(atomic::Ordering::Relaxed) {
            error!("received request to abort launch");
            instance.declare_status(InstanceStatus::InitFail);
            instance.send_status();
            return;
        }

        let tunnelkey_path = instance.wdeployment.ssh_key.clone().unwrap();

        if let Some(repo_info) = repo_args {
            let cprovider_execname = instance.wdeployment.cprovider.get_execname().unwrap();
            let status = Command::new(&cprovider_execname)
                .args([
                    "exec",
                    &name,
                    "/bin/sh",
                    "-c",
                    &format!("cd $HOME && git clone {} m", repo_info.url),
                ])
                .status();
            match status {
                Ok(clone_result) => {
                    if !clone_result.success() {
                        error!("clone of {repo_info:?} failed: {clone_result}");
                        instance.declare_status(InstanceStatus::InitFail);
                        instance.send_status();
                        return;
                    }
                }
                Err(err) => {
                    error!("clone of {repo_info:?} failed: {err}");
                    instance.declare_status(InstanceStatus::InitFail);
                    instance.send_status();
                    return;
                }
            }

            if let Some(path) = repo_info.path {
                let status = Command::new(cprovider_execname)
                    .args([
                        "exec",
                        &name,
                        "/bin/sh",
                        "-c",
                        &format!("cd $HOME/m && {path}"),
                    ])
                    .status();
                match status {
                    Ok(exec_result) => {
                        if !exec_result.success() {
                            error!("exec of {path} failed: {exec_result}");
                            instance.declare_status(InstanceStatus::InitFail);
                            instance.send_status();
                            return;
                        }
                    }
                    Err(err) => {
                        error!("exec of {path} failed: {err}");
                        instance.declare_status(InstanceStatus::InitFail);
                        instance.send_status();
                        return;
                    }
                }
            }
        }

        if let Err(err) = instance.start_sshtun(container_addr, &tunnelkey_path, 30) {
            error!("{err}");
            instance.declare_status(InstanceStatus::InitFail);
            instance.send_status();
            return;
        }

        instance.declare_status(InstanceStatus::Ready);
        instance.send_status();
    }

    fn terminate(&mut self) -> Result<(), String> {
        let mut status = self
            .status
            .lock()
            .expect("Instance status lock can be held");
        match &*status {
            Some(s) => {
                if s == &InstanceStatus::Terminating {
                    return Ok(());
                }
                if s == &InstanceStatus::Init || s == &InstanceStatus::InitFail {
                    warn!("received terminate request when {s}");
                    return Err(format!("cannot terminate when status is {s}"));
                }
                *status = Some(InstanceStatus::Terminating);
            }
            None => {
                return Err("terminate() called when no active instance".into());
            }
        }

        let instance = self.clone();
        thread::spawn(move || {
            CurrentInstance::destroy(instance);
        });
        Ok(())
    }

    fn send_destroy_done(&self) {
        if let Some(main_actor_addr) = &self.main_actor_addr {
            main_actor_addr.do_send(api::ClientWorkerMessage {
                mtype: CWorkerMessageType::WsSend,
                body: Some(
                    serde_json::to_string(&json!({
                        "v": 0,
                        "cmd": "ACK",
                        "req": "INSTANCE_DESTROY",
                        "st": "DONE",
                    }))
                    .expect("Instance destroy ACK message can be serialized to JSON"),
                ),
            });
        }
    }

    fn stop_tunnel(&self) {
        let mut tunnel_ref = self.tunnel.lock().expect("Lock on tunnel info can be held");
        if let Some(tunnel) = tunnel_ref.as_mut() {
            debug!("killing ssh tunnel process: {:?}", tunnel.proc);
            if let Err(err) = tunnel.proc.kill() {
                warn!("tunnel kill: : {err}");
            }
            match tunnel.proc.wait() {
                Ok(s) => {
                    if !s.success() {
                        warn!("exit code: {:?}", s.code());
                    }
                }
                Err(err) => {
                    error!("{err}");
                }
            }

            if self.wdeployment.cprovider == CProvider::Proxy {
                if let Some(subprocess) = tunnel.container_addr.subprocess.as_mut() {
                    debug!("killing proxy process: {subprocess:?}");
                    if let Err(err) = subprocess.kill() {
                        warn!("proxy kill: : {err}");
                    }
                    match subprocess.wait() {
                        Ok(s) => {
                            if !s.success() {
                                warn!("exit code: {:?}", s.code());
                            }
                        }
                        Err(err) => {
                            error!("{err}");
                        }
                    }
                }
            }
        }
        *tunnel_ref = None;
    }

    fn destroy(mut instance: CurrentInstance) {
        instance.stop_tunnel();

        let name = instance
            .get_local_name()
            .expect("Container name should be known during destroy process");
        if let Err(err) = Self::destroy_container(&instance.wdeployment, &name) {
            error!("Deployment fault! Caught from destroy_container(): {err}");
            instance.declare_status(InstanceStatus::Fault);
            return;
        }

        instance.clear_status();
        instance.send_destroy_done();
    }

    pub fn launch_container(
        wdeployment: &WDeployment,
        name: &str,
        public_key: &str,
    ) -> Result<ContainerAddress, Box<dyn std::error::Error>> {
        let cprovider = wdeployment.cprovider.clone();
        let ip: String;
        let port: Port;
        let hostkey: String;
        let mut subprocess = None;
        if cprovider == CProvider::Docker
            || cprovider == CProvider::DockerRootless
            || cprovider == CProvider::Podman
        {
            let cprovider_execname = cprovider.get_execname().unwrap();
            let image = match &wdeployment.image {
                Some(img) => img.clone(),
                None => {
                    return Err(Error::new("no image in configuration"));
                }
            };

            let mut run_command = Command::new(&cprovider_execname);
            let mut run_command = run_command.args([
                "run",
                "-d",
                "-h",
                name,
                "--name",
                name,
                "--device=/dev/net/tun:/dev/net/tun",
                "--cap-add=NET_ADMIN",
            ]);
            if cprovider != CProvider::Docker {
                run_command = run_command.args(["--cap-add=CAP_SYS_CHROOT"]);
            }
            run_command = run_command.args(&wdeployment.cargs);
            if cprovider == CProvider::Podman || cprovider == CProvider::DockerRootless {
                run_command = run_command.args(["-p", "127.0.0.1::22"]);
            }
            if log_enabled!(Level::Debug) {
                run_command = run_command.args(["-e", "HARDSHARE_LOG=1"])
            }
            run_command = run_command.arg(image);
            let command_result = match run_command.output() {
                Ok(o) => o,
                Err(err) => {
                    return Err(Error::new(format!("{err}")));
                }
            };
            if !command_result.status.success() {
                return Err(Error::new(format!(
                    "run command failed: {command_result:?}"
                )));
            }

            ip = if cprovider == CProvider::Podman || cprovider == CProvider::DockerRootless {
                "127.0.0.1".into()
            } else {
                match CurrentInstance::get_container_addr(&cprovider, name, 10) {
                    Ok(a) => a,
                    Err(err) => {
                        return Err(Error::new(err));
                    }
                }
            };

            port = if cprovider == CProvider::Docker {
                22
            } else {
                match CurrentInstance::get_container_sshport(&cprovider, name) {
                    Ok(a) => a,
                    Err(err) => {
                        return Err(Error::new(err));
                    }
                }
            };

            let mut public_key_file = match NamedTempFile::new() {
                Ok(f) => f,
                Err(err) => {
                    return Err(Error::new(err));
                }
            };
            match write!(public_key_file, "{public_key}") {
                Ok(()) => {
                    debug!(
                        "wrote public key file: {}",
                        public_key_file.path().to_string_lossy()
                    );
                }
                Err(err) => {
                    return Err(Error::new(format!(
                        "failed to write public key file ({}): {:?}",
                        public_key_file.path().to_string_lossy(),
                        err
                    )));
                }
            };

            let mkdir_result = Command::new(&cprovider_execname)
                .args(["exec", name, "/bin/mkdir", "-p", "/root/.ssh"])
                .status()
                .unwrap();
            if !mkdir_result.success() {
                return Err(Error::new(format!(
                    "mkdir command failed: {mkdir_result:?}"
                )));
            }

            let cp_result = Command::new(&cprovider_execname)
                .args([
                    "cp",
                    public_key_file.path().to_str().unwrap(),
                    &(name.to_string() + ":/root/.ssh/authorized_keys"),
                ])
                .status()
                .unwrap();
            if !cp_result.success() {
                return Err(Error::new(format!("cp command failed: {cp_result:?}")));
            }

            let chown_result = Command::new(&cprovider_execname)
                .args([
                    "exec",
                    name,
                    "/bin/chown",
                    "0:0",
                    "/root/.ssh/authorized_keys",
                ])
                .status()
                .unwrap();
            if !chown_result.success() {
                return Err(Error::new(format!(
                    "chown command failed: {chown_result:?}"
                )));
            }

            hostkey = match CurrentInstance::get_container_hostkey(&cprovider, name, 20) {
                Ok(k) => k,
                Err(err) => {
                    return Err(Error::new(err));
                }
            };

            for script in wdeployment.init_inside.iter() {
                let status = Command::new(&cprovider_execname)
                    .args(["exec", name, "/bin/sh", "-c", script])
                    .status();
                match status {
                    Ok(script_result) => {
                        if !script_result.success() {
                            return Err(Error::new(format!("`{script}` failed: {script_result}")));
                        }
                    }
                    Err(err) => {
                        return Err(Error::new(format!("`{script}` failed: {err}")));
                    }
                }
            }
        } else if cprovider == CProvider::Lxd {
            return Err(Error::new("lxd cprovider not implemented yet"));
        } else if cprovider == CProvider::Proxy {
            let res = CurrentInstance::start_proxy(&wdeployment.cargs, 5)?;
            port = res.1;
            ip = "127.0.0.1".into();
            hostkey = "".into();
            subprocess = Some(res.0);
        } else {
            return Err(Error::new(format!("unknown cprovider: {cprovider}")));
        }

        Ok(ContainerAddress {
            ip,
            port,
            hostkey,
            subprocess,
        })
    }

    pub fn destroy_container(
        wdeployment: &WDeployment,
        name: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if wdeployment.cprovider == CProvider::Docker
            || wdeployment.cprovider == CProvider::DockerRootless
            || wdeployment.cprovider == CProvider::Podman
        {
            let cprovider_execname = wdeployment.cprovider.get_execname().unwrap();
            let mut run_command = Command::new(cprovider_execname);
            let run_command = run_command.args(["rm", "-f", name]).stdout(Stdio::null());
            match run_command.status() {
                Ok(s) => {
                    if !s.success() {
                        return Err(Error::new(format!(
                            "exit code from {}: {:?}",
                            wdeployment.cprovider.get_execname().unwrap(),
                            s.code()
                        )));
                    }
                }
                Err(err) => {
                    return Err(Error::new(err));
                }
            }
        }

        for script in wdeployment.terminate.iter() {
            match Command::new("/bin/sh").args(["-c", script]).status() {
                Ok(script_result) => {
                    if !script_result.success() {
                        return Err(Error::new(format!("`{script}` failed: {script_result}")));
                    }
                }
                Err(err) => {
                    return Err(Error::new(format!("`{script}` failed: {err}")));
                }
            }
        }

        Ok(())
    }
} // impl CurrentInstance

pub fn cworker(
    wsclient_req: mpsc::Receiver<CWorkerCommand>,
    main_actor_addr: Addr<api::MainActor>,
    wdeployment: Arc<WDeployment>,
) {
    let mut current_instance = CurrentInstance::new(&wdeployment, Some(&main_actor_addr));

    loop {
        let req = match wsclient_req.recv() {
            Ok(m) => m,
            Err(_) => return,
        };
        debug!("cworker rx: {req:?}");

        match req.command {
            CWorkerCommandType::InstanceLaunch => {
                match current_instance.init(
                    &req.instance_id,
                    req.conntype.unwrap(),
                    &req.publickey.unwrap(),
                    req.repo_args,
                ) {
                    Ok(_) => {
                        main_actor_addr.do_send(api::ClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "ACK",
                                    "mi": req.message_id,
                                }))
                                .unwrap(),
                            ),
                        });
                    }
                    Err(err) => {
                        error!(
                            "launch request for instance {} failed: {}",
                            req.instance_id, err
                        );
                        main_actor_addr.do_send(api::ClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "NACK",
                                    "mi": req.message_id,
                                }))
                                .unwrap(),
                            ),
                        });
                    }
                };
            }
            CWorkerCommandType::InstanceDestroy => {
                if current_instance.exists() {
                    let status = current_instance.status().unwrap();
                    if status == InstanceStatus::Terminating {
                        // Already terminating; ACK but no action
                        warn!("destroy request received when already terminating");
                    } else if status != InstanceStatus::Ready {
                        warn!("destroy request received when status is {status}");
                        main_actor_addr.do_send(api::ClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "NACK",
                                    "mi": req.message_id,
                                }))
                                .unwrap(),
                            ),
                        });
                        continue;
                    }
                    main_actor_addr.do_send(api::ClientWorkerMessage {
                        mtype: CWorkerMessageType::WsSend,
                        body: Some(
                            serde_json::to_string(&json!({
                                "v": 0,
                                "cmd": "ACK",
                                "mi": req.message_id,
                            }))
                            .unwrap(),
                        ),
                    });
                    if status != InstanceStatus::Terminating {
                        if let Err(err) = current_instance.terminate() {
                            error!(
                                "terminate request for instance {} failed: {}",
                                &req.instance_id, err
                            );
                        }
                    }
                } else {
                    error!("destroy request received when there is no active instance");
                    main_actor_addr.do_send(api::ClientWorkerMessage {
                        mtype: CWorkerMessageType::WsSend,
                        body: Some(
                            serde_json::to_string(&json!({
                                "v": 0,
                                "cmd": "NACK",
                                "mi": req.message_id,
                            }))
                            .unwrap(),
                        ),
                    });
                }
            }
            CWorkerCommandType::InstanceStatus => {
                match current_instance.status() {
                    Some(status) => {
                        main_actor_addr.do_send(api::ClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "ACK",
                                    "s": status.to_string(),
                                    "mi": req.message_id,
                                }))
                                .unwrap(),
                            ),
                        });
                    }
                    None => {
                        warn!("status check received when there is no active instance");
                        main_actor_addr.do_send(api::ClientWorkerMessage {
                            mtype: CWorkerMessageType::WsSend,
                            body: Some(
                                serde_json::to_string(&json!({
                                    "v": 0,
                                    "cmd": "NACK",
                                    "mi": req.message_id,
                                }))
                                .unwrap(),
                            ),
                        });
                    }
                };
            }
            CWorkerCommandType::CreateSshTunDone => {
                if current_instance.exists() {
                    if let Err(err) = current_instance.handle_response(&req) {
                        error!("command CREATE_SSHTUN_DONE: {err}");
                    }
                } else {
                    error!("CREATE_SSHTUN_DONE received when there is no active instance");
                }
            }
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
enum CWorkerCommandType {
    InstanceLaunch,
    InstanceDestroy,
    InstanceStatus,
    CreateSshTunDone,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TunnelInfo {
    hostkey: String,
    ipv4: String,
    port: Port,
    thport: Port,
    thuser: String,
}

#[derive(Debug, Clone)]
pub struct RepoInfo {
    url: String,
    path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CWorkerCommand {
    command: CWorkerCommandType,
    instance_id: String, // \in UUID
    conntype: Option<ConnType>,
    publickey: Option<String>,
    tunnelinfo: Option<TunnelInfo>,
    message_id: Option<String>,
    repo_args: Option<RepoInfo>,
}

impl CWorkerCommand {
    pub fn get_status(instance_id: &str, message_id: &str) -> CWorkerCommand {
        CWorkerCommand {
            command: CWorkerCommandType::InstanceStatus,
            instance_id: String::from(instance_id),
            conntype: None,
            publickey: None,
            tunnelinfo: None,
            message_id: Some(String::from(message_id)),
            repo_args: None,
        }
    }

    pub fn launch_instance(
        instance_id: &str,
        message_id: &str,
        conntype: ConnType,
        public_key: &str,
        repo_url: Option<&str>,
        repo_path: Option<&str>,
    ) -> CWorkerCommand {
        let repo_args = repo_url.map(|u| RepoInfo {
            url: u.to_string(),
            path: repo_path.map(|x| x.to_string()),
        });
        CWorkerCommand {
            command: CWorkerCommandType::InstanceLaunch,
            instance_id: String::from(instance_id),
            conntype: Some(conntype),
            publickey: Some(String::from(public_key)),
            tunnelinfo: None,
            message_id: Some(String::from(message_id)),
            repo_args,
        }
    }

    pub fn destroy_instance(instance_id: &str, message_id: &str) -> CWorkerCommand {
        CWorkerCommand {
            command: CWorkerCommandType::InstanceDestroy,
            instance_id: String::from(instance_id),
            conntype: None,
            publickey: None,
            tunnelinfo: None,
            message_id: Some(String::from(message_id)),
            repo_args: None,
        }
    }

    pub fn create_sshtun_done(
        instance_id: &str,
        message_id: &str,
        tunnelinfo: &TunnelInfo,
    ) -> Self {
        Self {
            command: CWorkerCommandType::CreateSshTunDone,
            instance_id: String::from(instance_id),
            conntype: None,
            publickey: None,
            tunnelinfo: Some(tunnelinfo.clone()),
            message_id: Some(String::from(message_id)),
            repo_args: None,
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum CWorkerMessageType {
    WsSend,
}

#[cfg(test)]
mod tests {
    use std::sync::{atomic, Arc};

    use super::{ConnType, CurrentInstance};
    use crate::mgmt::WDeployment;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn create_example_wdeployment() -> WDeployment {
        serde_json::from_str(
            r#"
            {
                "id": "68a1be97-9365-4007-b726-14c56bd69eef",
                "owner": "bilbo",
                "cprovider": "podman",
                "cargs": [],
                "image": "rerobots/hs-generic",
                "terminate": [],
                "init_inside": [],
                "container_name": "rrc"
            }"#,
        )
        .unwrap()
    }

    fn create_example_proxy_wdeployment() -> WDeployment {
        serde_json::from_str(
            r#"
            {
                "id": "8449a67a-fe0d-42b3-9f2d-89c9aa2e9410",
                "owner": "frodo",
                "cprovider": "proxy",
                "cargs": ["rrhttp", "127.0.0.1:8080"],
                "init_inside": [],
                "terminate": [],
                "container_name": "rrc"
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn cannot_init_when_busy() -> TestResult {
        let wdeployment = create_example_proxy_wdeployment();
        let instance_ids = [
            "e5fcf112-7af2-4d9f-93ce-b93f0da9144d",
            "0f2576b5-17d9-477e-ba70-f07142faa2d9",
        ];
        let mut current_instance = CurrentInstance::new(&Arc::new(wdeployment.clone()), None);
        let result = current_instance.init(instance_ids[0], ConnType::SshTun, "", None);
        assert!(result.is_ok());
        let (thread_handle, abort_launch) = result?;

        assert!(current_instance.exists());
        assert!(current_instance
            .init(instance_ids[1], ConnType::SshTun, "", None)
            .is_err());

        abort_launch.store(true, atomic::Ordering::Relaxed);
        thread_handle
            .join()
            .expect("Instance init thread should be join-able");
        let name = current_instance.get_local_name()?;
        if let Err(err) = CurrentInstance::destroy_container(&wdeployment, &name) {
            panic!("{}", err);
        }

        Ok(())
    }

    #[test]
    fn generated_local_name_random() -> TestResult {
        let wdeployment = create_example_wdeployment();
        let mut instance = CurrentInstance::new(&Arc::new(wdeployment), None);
        let first = instance.generate_local_name("base");
        let first_as_stored = instance.get_local_name()?;
        assert_eq!(first, first_as_stored);
        let second = instance.generate_local_name("base");
        assert_ne!(first, second);
        Ok(())
    }
}
