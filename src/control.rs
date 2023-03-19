// SCL <scott@rerobots.net>
// Copyright (C) 2023 rerobots, Inc.

use std::sync::mpsc;

use actix::prelude::*;

use crate::api;


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


#[derive(PartialEq, Debug, Clone)]
pub enum ConnType {
    SshTun,
}


#[derive(Debug)]
struct CurrentInstance {
    status: Option<InstanceStatus>,
    id: Option<String>,
}

impl CurrentInstance {
    fn new() -> CurrentInstance {
        CurrentInstance {
            status: None,
            id: None,
        }
    }

    fn init(&mut self, instance_id: &str) -> Result<(), &str> {
        if self.exists() {
            return Err("already current instance, cannot INIT new instance");
        }
        self.status = Some(InstanceStatus::Init);
        self.id = Some(instance_id.into());
        Ok(())
    }

    fn exists(&self) -> bool {
        self.status.is_some()
    }
}


pub fn cworker(
    ac: api::HSAPIClient,
    wsclient_req: mpsc::Receiver<CWorkerCommand>,
    wsclient_addr: Addr<api::WSClient>,
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
                match &current_instance.status {
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
        let mut current_instance = CurrentInstance::new();
        assert!(current_instance.init(instance_ids[0]).is_ok());
        assert!(current_instance.exists());
        assert!(current_instance.init(instance_ids[1]).is_err());
    }
}
