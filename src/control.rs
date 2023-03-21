// SCL <scott@rerobots.net>
// Copyright (C) 2023 rerobots, Inc.

use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::thread;

use actix::prelude::*;

use crate::api;


#[derive(PartialEq, Debug, Clone)]
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


#[derive(PartialEq, Debug, Clone)]
pub enum ConnType {
    SshTun,
}


#[derive(Clone)]
struct CurrentInstance {
    status: Arc<Mutex<Option<InstanceStatus>>>,
    id: Option<String>,
    wsclient_addr: Option<Addr<api::WSClient>>,
}

impl CurrentInstance {
    fn new(wsclient_addr: Option<&Addr<api::WSClient>>) -> CurrentInstance {
        CurrentInstance {
            status: Arc::new(Mutex::new(None)),
            id: None,
            wsclient_addr: wsclient_addr.cloned(),
        }
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

    fn init(&mut self, instance_id: &str) -> Result<(), &str> {
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
        thread::spawn(move || {
            CurrentInstance::launch(instance);
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
        *x = None;
    }


    fn launch(mut instance: CurrentInstance) {
        instance.declare_status(InstanceStatus::Ready);
        instance.send_status();
    }

    fn terminate(&mut self) -> Result<(), &str> {
        let mut status = self.status.lock().unwrap();
        match &*status {
            Some(s) => {
                if s == &InstanceStatus::Terminating {
                    return Ok(());
                }
                if s == &InstanceStatus::Init {
                    return Err("cannot terminate() when status is INIT");
                }
                *status = Some(InstanceStatus::Terminating);
            }
            None => {
                return Err("terminate() called when no active instance");
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

    fn destroy(mut instance: CurrentInstance) {
        instance.clear_status();
        instance.send_destroy_done();
    }
}


pub fn cworker(
    ac: api::HSAPIClient,
    wsclient_req: mpsc::Receiver<CWorkerCommand>,
    wsclient_addr: Addr<api::WSClient>,
    wd: HashMap<String, serde_json::Value>,
) {
    let mut current_instance = CurrentInstance::new(Some(&wsclient_addr));

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

#[derive(Debug, Clone)]
pub struct CWorkerCommand {
    command: CWorkerCommandType,
    instance_id: String, // \in UUID
    conntype: Option<ConnType>,
    publickey: Option<String>,
    message_id: Option<String>,
}

impl CWorkerCommand {
    pub fn get_status(instance_id: &str, message_id: &str) -> CWorkerCommand {
        CWorkerCommand {
            command: CWorkerCommandType::InstanceStatus,
            instance_id: String::from(instance_id),
            conntype: None,
            publickey: None,
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
            message_id: Some(String::from(message_id)),
        }
    }

    pub fn destroy_instance(instance_id: &str, message_id: &str) -> CWorkerCommand {
        CWorkerCommand {
            command: CWorkerCommandType::InstanceDestroy,
            instance_id: String::from(instance_id),
            conntype: None,
            publickey: None,
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
    use super::CurrentInstance;

    #[test]
    fn cannot_init_when_busy() {
        let instance_ids = vec![
            "e5fcf112-7af2-4d9f-93ce-b93f0da9144d",
            "0f2576b5-17d9-477e-ba70-f07142faa2d9",
        ];
        let mut current_instance = CurrentInstance::new(None);
        assert!(current_instance.init(instance_ids[0]).is_ok());
        assert!(current_instance.exists());
        assert!(current_instance.init(instance_ids[1]).is_err());
    }
}
