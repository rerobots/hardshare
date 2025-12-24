// Copyright (C) 2025 rerobots, Inc.

use std::process::Command;
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
use tokio::{signal, time};

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
    body: Option<String>,
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
                    return Err(format!("unsupported verb {word}").into());
                }
            } else if uri.is_none() {
                match word.find('?') {
                    Some(sep) => {
                        let (path, _) = word.split_at(sep);
                        uri = Some(String::from(path));
                    }
                    None => {
                        uri = Some(String::from(word));
                    }
                }
            } else if protocol_match {
                return Err("too many words on first line".into());
            } else if word == "HTTP/1.1" {
                protocol_match = true;
            } else {
                return Err(format!("unexpected protocol specifier {word}").into());
            }
        }
        let verb = match verb {
            Some(v) => v,
            None => {
                return Err("no request verb".into());
            }
        };
        let uri = match uri {
            Some(u) => u,
            None => {
                return Err("no request URI".into());
            }
        };
        if !protocol_match {
            return Err("no valid protocol string".into());
        }
        let body = if body_start < blob.len() {
            if verb != HttpVerb::Post {
                warn!("Request has body, but verb does not allow it. Ignoring the body.");
                None
            } else {
                Some(String::from_utf8_lossy(&blob[body_start..]).to_string())
            }
        } else {
            None
        };
        Ok(Request { verb, uri, body })
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RequestRule {
    verb: HttpVerb,
    uri: String,
    command: Vec<String>,
}

impl RequestRule {
    fn run(&self, req: &Request) -> Result<String, String> {
        let args = if self.command.len() > 1 || req.body.is_some() {
            let mut args: Vec<String> = self.command[1..].to_vec();
            if req.verb == HttpVerb::Post {
                if let Some(b) = &req.body {
                    args.push(b.clone());
                }
            }
            args
        } else {
            vec![]
        };
        let output = match Command::new(&self.command[0]).args(args).output() {
            Ok(o) => o,
            Err(err) => {
                return Err(format!("{err}"));
            }
        };
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }
}

#[derive(Clone, Debug, Deserialize)]
struct Config {
    rules: Vec<RequestRule>,
}

impl Config {
    fn new() -> Self {
        Config { rules: vec![] }
    }

    fn new_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config: Config = serde_yaml::from_slice(&std::fs::read(path)?)?;
        for rule in config.rules.iter() {
            if rule.command.is_empty() {
                return Err(format!("{} {} has empty command", rule.verb, rule.uri).into());
            }
        }
        Ok(config)
    }

    fn find_rule(&self, req: &Request) -> Option<&RequestRule> {
        self.rules
            .iter()
            .find(|&r| r.verb == req.verb && r.uri == req.uri)
    }
}

fn wrap_output(out: &str) -> String {
    let content_length = out.len();
    if content_length > 0 {
        format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {content_length}\r\n\r\n{out}")
    } else {
        "HTTP/1.1 200 OK\r\n\r\n".into()
    }
}

async fn handle_request(config: Arc<Config>, mut ingress: TcpStream) {
    let ingress_peer_addr = ingress
        .peer_addr()
        .expect("Ingress address should be valid IPv4 or IPv6 socket address");
    debug!("handling RPC at {ingress_peer_addr}");
    let prefix = format!("{ingress_peer_addr}");

    let req;
    let mut buf = [0; 1024];
    let forbidden_response = "HTTP/1.1 403 Forbidden\r\n\r\n".as_bytes();

    loop {
        let n = match ingress.read(&mut buf).await {
            Ok(s) => s,
            Err(err) => {
                error!("{prefix}: error on read: {err}");
                return;
            }
        };
        if n == 0 {
            warn!("{prefix}: read 0 bytes; exiting...");
            return;
        }
        debug!("{prefix}: read {n} bytes");
        match Request::new(&buf[..n]) {
            Ok(r) => {
                debug!("parsed request: {r:?}");
                req = r;
                break;
            }
            Err(err) => {
                warn!("{err}");
            }
        };
    }

    let rule = match config.find_rule(&req) {
        Some(r) => r,
        None => {
            warn!("Request does not satisfy specification. Rejecting.");
            match ingress.write(forbidden_response).await {
                Ok(n) => {
                    debug!("wrote {n} bytes to ingress");
                }
                Err(err) => {
                    error!("while writing to ingress, error: {err}");
                    return;
                }
            }
            return;
        }
    };

    match rule.run(&req) {
        Ok(out) => match ingress.write(wrap_output(&out).as_bytes()).await {
            Ok(n) => {
                debug!("wrote {n} bytes to ingress");
            }
            Err(err) => {
                error!("while writing to ingress, error: {err}");
            }
        },
        Err(err) => {
            warn!("Procedure failed: {err}");
            match ingress.write(forbidden_response).await {
                Ok(n) => {
                    debug!("wrote {n} bytes to ingress");
                }
                Err(err) => {
                    error!("while writing to ingress, error: {err}");
                }
            }
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let matches = clap::App::new("rrpc")
        .max_term_width(80)
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
    debug!("Using configuration: {config:?}");

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
                        error!("error on accept connection: {err}; sleeping and looping...");
                        time::sleep(std::time::Duration::from_millis(1000)).await;
                        continue;
                    }
                };
                match ingress.set_nodelay(true) {
                    Ok(()) => (),
                    Err(err) => {
                        warn!("unable to set TCP NODELAY on ingress: {err}")
                    }
                };

                tokio::spawn(handle_request(config.clone(), ingress));
            }
        });

        signal::ctrl_c().await?;

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::{Config, HttpVerb, Request, RequestRule};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_empty_default() {
        let config = Config::new();

        // No rules => reject
        assert_eq!(
            config.find_rule(&Request {
                verb: HttpVerb::Get,
                uri: "/".into(),
                body: None,
            }),
            None
        );
    }

    #[test]
    fn test_simple_rules() -> TestResult {
        let mut config = Config::new();
        config.rules.push(RequestRule {
            verb: HttpVerb::Get,
            uri: "/date".into(),
            command: vec!["date".into()],
        });

        let mut req = Request {
            verb: HttpVerb::Get,
            uri: "/date".into(),
            body: None,
        };

        let rule = config.find_rule(&req);
        assert!(rule.is_some());
        let result = rule.expect("Request should have a matching rule").run(&req);
        assert!(result.is_ok());
        // "20" should appear in the year of any date string for next 74 years
        assert!(result?.contains("20"));

        req.verb = HttpVerb::Post;
        assert_eq!(config.find_rule(&req), None);

        Ok(())
    }
}
