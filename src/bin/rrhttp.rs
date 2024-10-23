// Copyright (C) 2024 rerobots, Inc.

use std::fmt::Write;
use std::sync::Arc;

#[macro_use]
extern crate clap;
use clap::Arg;

#[macro_use]
extern crate log;

extern crate serde;
use serde::Deserialize;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Builder;
use tokio::{signal, sync::mpsc, time};

#[derive(Clone, Debug, Deserialize, PartialEq)]
enum HttpVerb {
    #[serde(alias = "GET")]
    Get,

    #[serde(alias = "POST")]
    Post,
}

impl std::fmt::Display for HttpVerb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
        }
    }
}

#[derive(Debug)]
struct Request {
    verb: HttpVerb,
    uri: String,
    body: Option<serde_json::Value>,
}

impl Request {
    fn new(blob: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut verb = None;
        let mut uri = None;
        let mut protocol_match = false;
        let mut request_line_end = 0;
        let mut body_start = 0;
        for k in 1..(blob.len() - 3) {
            if blob[k] == 0x0d && blob[k + 1] == 0x0a {
                if request_line_end == 0 {
                    request_line_end = k;
                }
                if blob[k + 2] == 0x0d && blob[k + 3] == 0x0a {
                    body_start = k + 4;
                    break;
                }
            }
        }
        if request_line_end == 0 || body_start == 0 {
            return Err("request not well formed".into());
        }
        for word in String::from_utf8_lossy(&blob[..request_line_end]).split_whitespace() {
            if verb.is_none() {
                if word == "GET" {
                    verb = Some(HttpVerb::Get);
                } else if word == "POST" {
                    verb = Some(HttpVerb::Post);
                } else {
                    return Err(format!("unsupported verb {}", word).into());
                }
            } else if uri.is_none() {
                uri = Some(String::from(word));
            } else if protocol_match {
                return Err("too many words on first line".into());
            } else if word == "HTTP/1.1" {
                protocol_match = true;
            } else {
                return Err(format!("unexpected protocol specifier {}", word).into());
            }
        }
        if verb.is_none() {
            return Err("no request verb".into());
        }
        if uri.is_none() {
            return Err("no request URI".into());
        }
        if !protocol_match {
            return Err("no valid protocol string".into());
        }
        let body = if body_start < blob.len() {
            match serde_json::from_str(&String::from_utf8_lossy(&blob[body_start..])) {
                Ok(s) => Some(s),
                Err(err) => return Err(format!("error parsing body as JSON: {}", err).into()),
            }
        } else {
            None
        };
        Ok(Request {
            verb: verb.unwrap(),
            uri: uri.unwrap(),
            body,
        })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RequestRule {
    verb: HttpVerb,
    uri: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum ConfigMode {
    Block,
    Allow,
}

#[derive(Clone, Debug, Deserialize)]
struct Config {
    default: ConfigMode,
    rules: Vec<RequestRule>,
}

impl Config {
    fn new() -> Self {
        Config {
            default: ConfigMode::Allow,
            rules: vec![],
        }
    }

    fn new_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        Ok(serde_yaml::from_slice(&std::fs::read(path)?)?)
    }

    fn is_valid(&self, req: &Request) -> bool {
        let mut matched = false;
        for rule in self.rules.iter() {
            if req.verb == rule.verb && req.uri == rule.uri {
                if self.default == ConfigMode::Allow {
                    return false;
                } else {
                    matched = true;
                    break;
                }
            }
        }
        (self.default == ConfigMode::Allow && !matched)
            || (self.default == ConfigMode::Block && matched)
    }
}

async fn writer_job(mut rx: mpsc::Receiver<Vec<u8>>, mut sink: tokio::net::tcp::OwnedWriteHalf) {
    while let Some(blob) = rx.recv().await {
        match sink.write(&blob).await {
            Ok(n) => {
                debug!("wrote {} bytes to ingress", n);
            }
            Err(err) => {
                error!("while writing to ingress, error: {}", err);
                return;
            }
        }
    }
}

async fn filter_responses(
    prefix: String,
    mut x: tokio::net::tcp::OwnedReadHalf,
    ingress_writer: mpsc::Sender<Vec<u8>>,
) {
    let mut buf = [0; 1024];
    loop {
        let n = x.read(&mut buf).await.unwrap();
        if n == 0 {
            warn!("{}: read 0 bytes; exiting...", prefix);
            return;
        }
        debug!("{}: read {} bytes", prefix, n);
        let mut raw = String::new();
        for el in buf.iter().take(n - 1) {
            match write!(&mut raw, "{:02X} ", el) {
                Ok(()) => (),
                Err(err) => {
                    error!("{}: error on write: {}", prefix, err);
                    return;
                }
            }
        }
        match write!(&mut raw, "{:02X}", buf[n - 1]) {
            Ok(()) => (),
            Err(err) => {
                error!("{}: error on write: {}", prefix, err);
                return;
            }
        }
        debug!("{}: raw: {}", prefix, raw);

        ingress_writer.send(buf[..n].to_vec()).await.unwrap();
    }
}

async fn filter_requests(
    config: Arc<Config>,
    prefix: String,
    mut x: tokio::net::tcp::OwnedReadHalf,
    mut y: tokio::net::tcp::OwnedWriteHalf,
    ingress_writer: mpsc::Sender<Vec<u8>>,
) {
    let mut buf = [0; 1024];
    let forbidden_response = "HTTP/1.1 403 Forbidden\r\n\r\n".as_bytes();
    loop {
        let n = x.read(&mut buf).await.unwrap();
        if n == 0 {
            warn!("{}: read 0 bytes; exiting...", prefix);
            return;
        }
        debug!("{}: read {} bytes", prefix, n);
        let req = match Request::new(&buf[..n]) {
            Ok(r) => r,
            Err(err) => {
                warn!("{}", err);
                continue;
            }
        };
        debug!("parsed request: {:?}", req);
        if !config.is_valid(&req) {
            warn!("Request does not satisfy specification. Rejecting.");
            ingress_writer
                .send(forbidden_response.to_vec())
                .await
                .unwrap();
            return;
        }
        match y.write(&buf[..n]).await {
            Ok(n) => {
                debug!("{}: wrote {} bytes", prefix, n);
            }
            Err(err) => {
                error!("{}: error on write: {}", prefix, err);
                return;
            }
        }
    }
}

async fn main_per(config: Arc<Config>, ingress: TcpStream, egress: TcpStream) {
    let ingress_peer_addr = ingress.peer_addr().unwrap();
    let egress_peer_addr = egress.peer_addr().unwrap();
    debug!(
        "started filtering {} to {}",
        ingress_peer_addr, egress_peer_addr
    );
    let (ingress_read, ingress_write) = ingress.into_split();
    let (egress_read, egress_write) = egress.into_split();
    let (tx, rx) = mpsc::channel(100);
    let ingress_writer_task = tokio::spawn(writer_job(rx, ingress_write));
    let in_to_e = tokio::spawn(filter_requests(
        config,
        format!("{} to {}", ingress_peer_addr, egress_peer_addr),
        ingress_read,
        egress_write,
        tx.clone(),
    ));
    let e_to_in = tokio::spawn(filter_responses(
        format!("{} to {}", egress_peer_addr, ingress_peer_addr),
        egress_read,
        tx,
    ));
    if let Err(err) = in_to_e.await {
        error!("{:?}", err);
    }
    if let Err(err) = e_to_in.await {
        error!("{:?}", err);
    }
    if let Err(err) = ingress_writer_task.await {
        error!("{:?}", err)
    }
    debug!("done");
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let matches = clap::App::new("rrhttp")
        .max_term_width(80)
        .arg(
            Arg::with_name("TARGET")
                .required(true)
                .help("target HOST:PORT"),
        )
        .arg(
            Arg::with_name("config")
                .long("config")
                .value_name("FILE")
                .help("configuration file"),
        )
        .version(crate_version!())
        .get_matches();

    let config = Arc::new(match matches.value_of("config") {
        Some(path) => Config::new_from_file(path)?,
        None => Config::new(),
    });

    let targetaddr = String::from(matches.value_of("TARGET").unwrap());

    let rt = Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()?;
    rt.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        println!("{}", listener.local_addr()?);

        tokio::spawn(async move {
            loop {
                let (ingress, _) = match listener.accept().await {
                    Ok(x) => x,
                    Err(err) => {
                        error!(
                            "error on accept connection: {}; sleeping and looping...",
                            err
                        );
                        time::sleep(std::time::Duration::from_millis(1000)).await;
                        continue;
                    }
                };
                match ingress.set_nodelay(true) {
                    Ok(()) => (),
                    Err(err) => {
                        warn!("unable to set TCP NODELAY on ingress: {}", err)
                    }
                };

                let egress = match TcpStream::connect(targetaddr.clone()).await {
                    Ok(c) => c,
                    Err(err) => {
                        error!("unable to connect to target: {}", err);
                        continue;
                    }
                };
                match egress.set_nodelay(true) {
                    Ok(()) => (),
                    Err(err) => {
                        warn!("unable to set TCP NODELAY on egress: {}", err)
                    }
                };

                tokio::spawn(main_per(config.clone(), ingress, egress));
            }
        });

        signal::ctrl_c().await?;

        Ok(())
    })
}
