// Copyright (C) 2024 rerobots, Inc.

use std::fmt::Write;

#[macro_use]
extern crate clap;
use clap::Arg;

#[macro_use]
extern crate log;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Builder;
use tokio::{signal, time};

#[derive(Debug, PartialEq)]
enum HttpVerb {
    Get,
    Post,
}

#[derive(Debug)]
struct Request {
    verb: HttpVerb,
    path: String,
    body: Option<serde_json::Value>,
}

impl Request {
    fn new(blob: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut verb = None;
        let mut path = None;
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
            } else if path.is_none() {
                path = Some(String::from(word));
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
        if path.is_none() {
            return Err("no request path".into());
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
            path: path.unwrap(),
            body,
        })
    }
}

enum ConfigMode {
    DefaultBlock,
    DefaultAllow,
}

struct Config {
    mode: ConfigMode,
    permitlist: Vec<Request>,
}

impl Config {
    fn new() -> Self {
        Config {
            mode: ConfigMode::DefaultAllow,
            permitlist: vec![],
        }
    }

    fn new_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {}
}

async fn x_to_y_nofilter(
    prefix: String,
    mut x: tokio::net::tcp::OwnedReadHalf,
    mut y: tokio::net::tcp::OwnedWriteHalf,
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

async fn x_to_y_filter(
    prefix: String,
    mut x: tokio::net::tcp::OwnedReadHalf,
    mut y: tokio::net::tcp::OwnedWriteHalf,
) {
    let mut buf = [0; 1024];
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

async fn main_per(ingress: TcpStream, egress: TcpStream) {
    let ingress_peer_addr = ingress.peer_addr().unwrap();
    let egress_peer_addr = egress.peer_addr().unwrap();
    debug!(
        "started filtering {} to {}",
        ingress_peer_addr, egress_peer_addr
    );
    let (ingress_read, ingress_write) = ingress.into_split();
    let (egress_read, egress_write) = egress.into_split();
    let in_to_e = tokio::spawn(x_to_y_filter(
        format!("{} to {}", ingress_peer_addr, egress_peer_addr),
        ingress_read,
        egress_write,
    ));
    let e_to_in = tokio::spawn(x_to_y_nofilter(
        format!("{} to {}", egress_peer_addr, ingress_peer_addr),
        egress_read,
        ingress_write,
    ));
    if let Err(err) = in_to_e.await {
        error!("{:?}", err);
    }
    if let Err(err) = e_to_in.await {
        error!("{:?}", err);
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

                tokio::spawn(main_per(ingress, egress));
            }
        });

        signal::ctrl_c().await?;

        Ok(())
    })
}
