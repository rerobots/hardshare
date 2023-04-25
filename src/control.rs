// SCL <scott@rerobots.net>
// Copyright (C) 2023 rerobots, Inc.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::File;
use std::io::prelude::*;
use std::io::Write;
use std::process::Command;
use std::str::FromStr;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use actix::prelude::*;
use serde::Deserialize;
use tempfile::NamedTempFile;

use crate::api;
use crate::mgmt::WDeployment;


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


struct SshTunnel {
    proc: std::process::Child,
    tunnelkey_path: std::path::PathBuf,
    container_addr: String,
    container_port: Port,
}


#[derive(Clone)]
struct CurrentInstance {
    wdeployment: Arc<WDeployment>,
    status: Arc<Mutex<Option<InstanceStatus>>>,
    id: Option<String>,
    local_name: Arc<Mutex<Option<String>>>,
    wsclient_addr: Option<Addr<api::WSClient>>,
    responses: Arc<Mutex<HashMap<String, Option<CWorkerCommand>>>>,
    tunnel: Arc<Mutex<Option<SshTunnel>>>,
}

impl CurrentInstance {
    fn new(
        wdeployment: &Arc<WDeployment>,
        wsclient_addr: Option<&Addr<api::WSClient>>,
    ) -> CurrentInstance {
        CurrentInstance {
            wdeployment: Arc::clone(wdeployment),
            status: Arc::new(Mutex::new(None)),
            id: None,
            local_name: Arc::new(Mutex::new(None)),
            wsclient_addr: wsclient_addr.cloned(),
            responses: Arc::new(Mutex::new(HashMap::new())),
            tunnel: Arc::new(Mutex::new(None)),
        }
    }

    fn generate_local_name(&mut self, base_name: &str) -> String {
        let random_suffix: String = rand::random::<u16>().to_string();
        let mut local_name = self.local_name.lock().unwrap();
        *local_name = Some(base_name.to_string() + &random_suffix);
        local_name.as_ref().unwrap().clone()
    }

    fn get_local_name(&self) -> Option<String> {
        let local_name = self.local_name.lock().unwrap();
        local_name.clone()
    }

    fn handle_response(&mut self, res: &CWorkerCommand) -> Result<(), String> {
        let message_id: String = res.message_id.clone().unwrap();
        let mut responses = self.responses.lock().unwrap();
        if !responses.contains_key(&message_id) {
            return Err(format!("unknown message {}", message_id));
        }
        if responses[&message_id].is_some() {
            return Err(format!("already handled message {}", message_id));
        }
        responses.insert(message_id, Some(res.clone()));
        Ok(())
    }

    fn send_status(&self) {
        if let Some(wsclient_addr) = &self.wsclient_addr {
            let status = self.status.lock().unwrap();
            match &*status {
                Some(s) => {
                    wsclient_addr.do_send(api::WSClientWorkerMessage {
                        mtype: CWorkerMessageType::WsSend,
                        body: Some(
                            serde_json::to_string(&json!({
                                "v": 0,
                                "cmd": "INSTANCE_STATUS",
                                "s": s.to_string(),
                            }))
                            .unwrap(),
                        ),
                    });
                }
                None => {
                    error!("called when no active instance");
                }
            }
        }
    }


    fn send_create_sshtun(&self, tunnelkey_public: &str) -> Result<String, String> {
        if let Some(wsclient_addr) = &self.wsclient_addr {
            let message_id = {
                let mut message_id: String;
                let mut responses = self.responses.lock().unwrap();
                loop {
                    message_id = rand::random::<u32>().to_string();
                    if !responses.contains_key(&message_id) {
                        responses.insert(message_id.clone(), None);
                        break;
                    }
                }
                message_id
            };
            let status = self.status.lock().unwrap();
            match &*status {
                Some(s) => {
                    wsclient_addr.do_send(api::WSClientWorkerMessage {
                        mtype: CWorkerMessageType::WsSend,
                        body: Some(
                            serde_json::to_string(&json!({
                                "v": 0,
                                "cmd": "CREATE_SSHTUN",
                                "id": self.id.as_ref().clone(),
                                "key": tunnelkey_public,
                                "mi": message_id,
                            }))
                            .unwrap(),
                        ),
                    });
                    Ok(message_id)
                }
                None => Err("called when no active instance".into()),
            }
        } else {
            Err("called without WebSocket client".into())
        }
    }


    fn init(&mut self, instance_id: &str, public_key: &str) -> Result<(), &str> {
        let mut status = self.status.lock().unwrap();
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
        thread::spawn(move || {
            CurrentInstance::launch(instance, &public_key);
        });
        Ok(())
    }

    fn exists(&self) -> bool {
        self.status.lock().unwrap().is_some()
    }

    fn status(&self) -> Option<InstanceStatus> {
        (*self.status.lock().unwrap()).as_ref().cloned()
    }

    fn declare_status(&mut self, new_status: InstanceStatus) {
        let mut x = self.status.lock().unwrap();
        *x = Some(new_status);
    }

    fn clear_status(&mut self) {
        let mut x = self.status.lock().unwrap();
        if *x != Some(InstanceStatus::Fault) {
            *x = None;
        }
    }


    fn get_container_addr(cprovider: &str, name: &str, timeout: u64) -> Result<String, String> {
        let max_duration = std::time::Duration::from_secs(timeout);
        let sleep_time = std::time::Duration::from_secs(2);
        let now = std::time::Instant::now();
        while now.elapsed() <= max_duration {
            let mut run_command = Command::new(cprovider);
            let run_command = run_command.args(["inspect", name]);
            let command_result = match run_command.output() {
                Ok(o) => o,
                Err(err) => return Err(format!("{}", err)),
            };
            if !command_result.status.success() {
                return Err(format!("run command failed: {:?}", command_result));
            }
            let r: serde_json::Value = match serde_json::from_slice(&command_result.stdout) {
                Ok(o) => o,
                Err(err) => return Err(format!("{}", err)),
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


    fn get_container_sshport(cprovider: &str, name: &str) -> Result<Port, String> {
        let mut run_command = Command::new(cprovider);
        let run_command = run_command.args(["port", name, "22"]);
        let command_result = match run_command.output() {
            Ok(o) => o,
            Err(err) => return Err(format!("{}", err)),
        };
        if !command_result.status.success() {
            return Err(format!("run command failed: {:?}", command_result));
        }

        let s = String::from_utf8(command_result.stdout).unwrap();
        let s = s.trim();
        let parts: Vec<&str> = s.split(':').collect();
        match Port::from_str(parts[1]) {
            Ok(port) => Ok(port),
            Err(err) => Err("SSH port not found".into()),
        }
    }


    fn get_container_hostkey(cprovider: &str, name: &str, timeout: u64) -> Result<String, String> {
        let hostkey_filename = "ssh_host_ecdsa_key.pub";
        let hostkey_contained_path = String::from(name) + ":/etc/ssh/" + hostkey_filename;
        let max_duration = std::time::Duration::from_secs(timeout);
        let sleep_time = std::time::Duration::from_secs(2);
        let now = std::time::Instant::now();
        while now.elapsed() <= max_duration {
            match Command::new(cprovider)
                .args(["cp", &hostkey_contained_path, "."])
                .status()
            {
                Ok(copy_result) => {
                    if copy_result.success() {
                        let mut hostkey_file = match File::open(hostkey_filename) {
                            Ok(f) => f,
                            Err(err) => return Err(format!("{}", err)),
                        };
                        let mut hostkey = String::new();
                        if let Err(err) = hostkey_file.read_to_string(&mut hostkey) {
                            return Err(format!("{}", err));
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


    fn start_sshtun(
        &self,
        container_addr: &str,
        container_port: Port,
        tunnelkey_path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let tunnelkey_public_path = String::from(tunnelkey_path) + ".pub";
        let mut f = File::open(tunnelkey_public_path)?;
        let mut tunnelkey_public = String::new();
        f.read_to_string(&mut tunnelkey_public)?;
        let message_id = self.send_create_sshtun(&tunnelkey_public)?;
        let st = std::time::Duration::from_secs(2);
        let tunnelinfo;
        loop {
            std::thread::sleep(st);
            {
                let responses = self.responses.lock().unwrap();
                if let Some(res) = &responses[&message_id] {
                    tunnelinfo = res.tunnelinfo.clone().unwrap();
                    break;
                }
            }
            info!("waiting for sshtun creation...");
        }
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
            &format!(":2210:{container_addr}:{container_port}"),
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
        println!("tunnel process args: {:?}", tunnel_process_args);
        let tunnel_process = Command::new("ssh").args(tunnel_process_args).spawn()?;

        let mut tunnel = self.tunnel.lock().unwrap();
        *tunnel = Some(SshTunnel {
            proc: tunnel_process,
            tunnelkey_path: tunnelkey_path.into(),
            container_addr: container_addr.into(),
            container_port,
        });
        Ok(())
    }


    fn launch(mut instance: CurrentInstance, public_key: &str) {
        let cprovider = instance.wdeployment.cprovider.clone();
        if cprovider == "docker" || cprovider == "podman" {
            let image = match &instance.wdeployment.image {
                Some(img) => img.clone(),
                None => {
                    error!("no image in configuration");
                    instance.declare_status(InstanceStatus::InitFail);
                    instance.send_status();
                    return;
                }
            };
            let base_name = instance.wdeployment.container_name.clone();
            let name = instance.generate_local_name(&base_name);
            let tunnelkey_path = instance.wdeployment.ssh_key.clone().unwrap();

            let mut run_command = Command::new(&cprovider);
            let mut run_command = run_command.args([
                "run",
                "-d",
                "-h",
                &name,
                "--name",
                &name,
                "--device=/dev/net/tun:/dev/net/tun",
                "--cap-add=NET_ADMIN",
                "--cap-add=CAP_SYS_CHROOT",
            ]);
            run_command = run_command.args(&instance.wdeployment.cargs);
            if cprovider == "podman" {
                run_command = run_command.args(["-p", "127.0.0.1::22"]);
            }
            run_command = run_command.arg(image);
            let command_result = match run_command.output() {
                Ok(o) => o,
                Err(err) => {
                    error!("{}", err);
                    instance.declare_status(InstanceStatus::InitFail);
                    instance.send_status();
                    return;
                }
            };
            if !command_result.status.success() {
                error!("run command failed: {:?}", command_result);
                instance.declare_status(InstanceStatus::InitFail);
                instance.send_status();
                return;
            }

            let addr: String = if cprovider == "podman" {
                "127.0.0.1".into()
            } else {
                match CurrentInstance::get_container_addr(&cprovider, &name, 10) {
                    Ok(a) => a,
                    Err(err) => {
                        error!("{}", err);
                        instance.declare_status(InstanceStatus::InitFail);
                        instance.send_status();
                        return;
                    }
                }
            };

            let sshport = if cprovider == "docker" {
                22
            } else {
                match CurrentInstance::get_container_sshport(&cprovider, &name) {
                    Ok(a) => a,
                    Err(err) => {
                        error!("{}", err);
                        instance.declare_status(InstanceStatus::InitFail);
                        instance.send_status();
                        return;
                    }
                }
            };

            let mut public_key_file = match NamedTempFile::new() {
                Ok(f) => f,
                Err(err) => {
                    error!("{}", err);
                    instance.declare_status(InstanceStatus::InitFail);
                    instance.send_status();
                    return;
                }
            };
            write!(public_key_file, "{}", public_key);

            let mkdir_result = Command::new(&cprovider)
                .args(["exec", &name, "/bin/mkdir", "-p", "/root/.ssh"])
                .status()
                .unwrap();
            if !mkdir_result.success() {
                error!("mkdir command failed: {:?}", mkdir_result);
                instance.declare_status(InstanceStatus::InitFail);
                instance.send_status();
                return;
            }

            let cp_result = Command::new(&cprovider)
                .args([
                    "cp",
                    public_key_file.path().to_str().unwrap(),
                    &(name.clone() + ":/root/.ssh/authorized_keys"),
                ])
                .status()
                .unwrap();
            if !cp_result.success() {
                error!("cp command failed: {:?}", cp_result);
                instance.declare_status(InstanceStatus::InitFail);
                instance.send_status();
                return;
            }

            let chown_result = Command::new(&cprovider)
                .args([
                    "exec",
                    &name,
                    "/bin/chown",
                    "0:0",
                    "/root/.ssh/authorized_keys",
                ])
                .status()
                .unwrap();
            if !chown_result.success() {
                error!("chown command failed: {:?}", chown_result);
                instance.declare_status(InstanceStatus::InitFail);
                instance.send_status();
                return;
            }

            let hostkey: String =
                match CurrentInstance::get_container_hostkey(&cprovider, &name, 10) {
                    Ok(k) => k,
                    Err(err) => {
                        error!("{}", err);
                        instance.declare_status(InstanceStatus::InitFail);
                        instance.send_status();
                        return;
                    }
                };

            for script in instance.wdeployment.init_inside.iter() {
                let status = Command::new(&cprovider)
                    .args(["exec", &name, "/bin/sh", "-c", script])
                    .status();
                match status {
                    Ok(script_result) => {
                        if !script_result.success() {
                            error!("`{script}` failed: {}", script_result);
                            instance.declare_status(InstanceStatus::InitFail);
                            instance.send_status();
                            return;
                        }
                    }
                    Err(err) => {
                        error!("`{script}` failed: {}", err);
                        instance.declare_status(InstanceStatus::InitFail);
                        instance.send_status();
                        return;
                    }
                }
            }

            if let Err(err) = instance.start_sshtun(&addr, sshport, &tunnelkey_path) {
                error!("{}", err);
                instance.declare_status(InstanceStatus::InitFail);
                instance.send_status();
                return;
            }
        } else if cprovider == "lxd" {
            error!("lxd cprovider not implemented yet");
            instance.declare_status(InstanceStatus::InitFail);
            instance.send_status();
            return;
        } else if cprovider == "proxy" {
            error!("proxy cprovider not implemented yet");
            instance.declare_status(InstanceStatus::InitFail);
            instance.send_status();
            return;
        } else {
            error!("unknown cprovider: {}", cprovider);
            instance.declare_status(InstanceStatus::InitFail);
            instance.send_status();
            return;
        }
        instance.declare_status(InstanceStatus::Ready);
        instance.send_status();
    }

    fn terminate(&mut self) -> Result<(), String> {
        let mut status = self.status.lock().unwrap();
        match &*status {
            Some(s) => {
                if s == &InstanceStatus::Terminating {
                    return Ok(());
                }
                if s == &InstanceStatus::Init || s == &InstanceStatus::InitFail {
                    warn!("received terminate request when {}", s);
                    return Err(format!("cannot terminate when status is {}", s));
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
        if let Some(wsclient_addr) = &self.wsclient_addr {
            wsclient_addr.do_send(api::WSClientWorkerMessage {
                mtype: CWorkerMessageType::WsSend,
                body: Some(
                    serde_json::to_string(&json!({
                        "v": 0,
                        "cmd": "ACK",
                        "req": "INSTANCE_DESTROY",
                        "st": "DONE",
                    }))
                    .unwrap(),
                ),
            });
        }
    }

    fn stop_tunnel(&self) {
        let mut tunnel_ref = self.tunnel.lock().unwrap();
        if let Some(tunnel) = tunnel_ref.as_mut() {
            if let Err(err) = tunnel.proc.kill() {
                warn!("tunnel kill: : {}", err);
            }
            match tunnel.proc.wait() {
                Ok(s) => {
                    if !s.success() {
                        warn!("exit code: {:?}", s.code());
                    }
                }
                Err(err) => {
                    error!("{}", err);
                }
            }
        }
        *tunnel_ref = None;
    }

    fn destroy(mut instance: CurrentInstance) {
        instance.stop_tunnel();

        if instance.wdeployment.cprovider == "docker" || instance.wdeployment.cprovider == "podman"
        {
            let name = instance.get_local_name().unwrap();
            let mut run_command = Command::new(&instance.wdeployment.cprovider);
            let mut run_command = run_command.args(["rm", "-f", &name]);
            match run_command.status() {
                Ok(s) => {
                    if !s.success() {
                        error!("exit code: {:?}", s.code());
                        instance.declare_status(InstanceStatus::Fault);
                        return;
                    }
                }
                Err(err) => {
                    error!("{}", err);
                    instance.declare_status(InstanceStatus::Fault);
                    return;
                }
            }
        }

        for script in instance.wdeployment.terminate.iter() {
            match Command::new("/bin/sh").args(["-c", script]).status() {
                Ok(script_result) => {
                    if !script_result.success() {
                        error!("`{script}` failed: {}", script_result);
                        instance.declare_status(InstanceStatus::Fault);
                        return;
                    }
                }
                Err(err) => {
                    error!("`{script}` failed: {}", err);
                    instance.declare_status(InstanceStatus::Fault);
                    return;
                }
            }
        }

        instance.clear_status();
        instance.send_destroy_done();
    }
}


pub fn cworker(
    ac: api::HSAPIClient,
    wsclient_req: mpsc::Receiver<CWorkerCommand>,
    wsclient_addr: Addr<api::WSClient>,
    wdeployment: Arc<WDeployment>,
) {
    let mut current_instance = CurrentInstance::new(&wdeployment, Some(&wsclient_addr));

    loop {
        let req = match wsclient_req.recv() {
            Ok(m) => m,
            Err(_) => return,
        };
        debug!("cworker rx: {:?}", req);

        match req.command {
            CWorkerCommandType::InstanceLaunch => {
                match current_instance.init(&req.instance_id, &req.publickey.unwrap()) {
                    Ok(()) => {
                        wsclient_addr.do_send(api::WSClientWorkerMessage {
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
                        wsclient_addr.do_send(api::WSClientWorkerMessage {
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
                    if status == InstanceStatus::Init {
                        warn!("destroy request received when status is INIT");
                        wsclient_addr.do_send(api::WSClientWorkerMessage {
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
                    if status == InstanceStatus::Terminating {
                        // Already terminating; ACK but no action
                        warn!("destroy request received when already terminating");
                    }
                    wsclient_addr.do_send(api::WSClientWorkerMessage {
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
                        current_instance.terminate();
                    }
                } else {
                    error!("destroy request received when there is no active instance");
                    wsclient_addr.do_send(api::WSClientWorkerMessage {
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
                        wsclient_addr.do_send(api::WSClientWorkerMessage {
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
                        wsclient_addr.do_send(api::WSClientWorkerMessage {
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
                    current_instance.handle_response(&req);
                } else {
                    error!("CREATE_SSHTUN_DONE received when there is no active instance");
                }
            }
            CWorkerCommandType::HubPing => {
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
    HubPing,
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
pub struct CWorkerCommand {
    command: CWorkerCommandType,
    instance_id: String, // \in UUID
    conntype: Option<ConnType>,
    publickey: Option<String>,
    tunnelinfo: Option<TunnelInfo>,
    message_id: Option<String>,
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
        }
    }

    pub fn launch_instance(
        instance_id: &str,
        message_id: &str,
        conntype: ConnType,
        public_key: &str,
    ) -> CWorkerCommand {
        CWorkerCommand {
            command: CWorkerCommandType::InstanceLaunch,
            instance_id: String::from(instance_id),
            conntype: Some(ConnType::SshTun),
            publickey: Some(String::from(public_key)),
            tunnelinfo: None,
            message_id: Some(String::from(message_id)),
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
        }
    }
}

#[derive(PartialEq, Debug, Clone)]
pub enum CWorkerMessageType {
    WsSend,
}


#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::CurrentInstance;


    #[test]
    fn cannot_init_when_busy() {
        let wdeployment = serde_json::from_str(
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
        .unwrap();
        let instance_ids = vec![
            "e5fcf112-7af2-4d9f-93ce-b93f0da9144d",
            "0f2576b5-17d9-477e-ba70-f07142faa2d9",
        ];
        let mut current_instance = CurrentInstance::new(&Arc::new(wdeployment), None);
        assert!(current_instance.init(instance_ids[0], "").is_ok());
        assert!(current_instance.exists());
        assert!(current_instance.init(instance_ids[1], "").is_err());
    }
}
