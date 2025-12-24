// Copyright (C) 2024 rerobots, Inc.

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::Arc;

#[macro_use]
extern crate clap;
use clap::Arg;

#[macro_use]
extern crate log;

#[macro_use]
extern crate serde_json;
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
    query: Option<HashMap<String, Option<String>>>,
}

impl Request {
    fn new(blob: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let mut verb = None;
        let mut uri = None;
        let mut protocol_match = false;
        let mut request_line_end = 0;
        let mut body_start = 0;
        let mut query = None;
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
                        let (path, qs) = word.split_at(sep);
                        uri = Some(String::from(path));
                        query = Some(Self::parse_query_string(&qs[1..]));
                    }
                    None => {
                        uri = Some(String::from(word));
                        query = None;
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
            match serde_json::from_str(&String::from_utf8_lossy(&blob[body_start..])) {
                Ok(s) => Some(s),
                Err(err) => return Err(format!("error parsing body as JSON: {err}").into()),
            }
        } else {
            None
        };
        Ok(Request {
            verb,
            uri,
            body,
            query,
        })
    }

    fn parse_query_string(qs: &str) -> HashMap<String, Option<String>> {
        let mut query = HashMap::new();
        for frag in qs.split('&') {
            match frag.find('=') {
                Some(sep) => {
                    let (k, v) = frag.split_at(sep);
                    query.insert(k.to_string(), Some(v[1..].to_string()));
                }
                None => {
                    query.insert(frag.to_string(), None);
                }
            }
        }
        query
    }

    pub fn satisfies(&self, rule: &RequestRule) -> bool {
        if self.verb != rule.verb || self.uri != rule.uri {
            return false;
        }
        if let Some(has_params) = rule.has_params {
            if has_params != self.query.is_some() {
                return false;
            }
        }
        if let Some(has_body) = rule.has_body {
            if has_body != self.body.is_some() {
                return false;
            }
        }
        match &rule.schema {
            Some(schema) => {
                if rule.verb == HttpVerb::Get {
                    let query = match &self.query {
                        Some(q) => q,
                        None => return true,
                    };
                    let mut matched = vec![];
                    for value_rule in schema {
                        let query_value = match query.get(&value_rule.name) {
                            Some(v_option) => match v_option {
                                Some(v) => {
                                    if matched.contains(&v) {
                                        // Reject if there are duplicates
                                        return false;
                                    }
                                    v
                                }
                                None => {
                                    // TODO: is empty parameter equivalent to `true`?
                                    return false;
                                }
                            },
                            None => {
                                let mut this_match = None;
                                if !value_rule.case_sensitive {
                                    let normalized_name = value_rule.name.to_lowercase();
                                    for (normalized_key, v_option) in query
                                        .iter()
                                        .map(|(k, v_option)| (k.to_lowercase(), v_option))
                                    {
                                        if normalized_key == normalized_name {
                                            match v_option {
                                                Some(v) => {
                                                    if matched.contains(&&value_rule.name) {
                                                        // Reject if there are duplicates
                                                        return false;
                                                    }
                                                    this_match = Some(v);
                                                    break;
                                                }
                                                None => {
                                                    // TODO: is empty parameter equivalent to `true`?
                                                    return false;
                                                }
                                            }
                                        }
                                    }
                                }
                                match this_match {
                                    Some(v) => v,
                                    None => {
                                        if !value_rule.optional {
                                            return false;
                                        }
                                        continue;
                                    }
                                }
                            }
                        };
                        matched.push(&value_rule.name);
                        match value_rule.value_type {
                            ValueType::Bool => {
                                if query_value != "true" && query_value != "false" {
                                    return false;
                                }
                            }
                            ValueType::Float => {
                                let parsed_val = match query_value.parse::<f32>() {
                                    Ok(v) => v,
                                    Err(err) => {
                                        warn!("caught while parsing query float value: {err}");
                                        return false;
                                    }
                                };
                                if let Some(range) = value_rule.range {
                                    if parsed_val < range.0.into() || parsed_val > range.1.into() {
                                        return false;
                                    }
                                }
                            }
                            ValueType::Int => {
                                let parsed_val = match query_value.parse::<i16>() {
                                    Ok(v) => v,
                                    Err(err) => {
                                        warn!("caught while parsing query int value: {err}");
                                        return false;
                                    }
                                };
                                if let Some(range) = value_rule.range {
                                    if parsed_val < range.0 || parsed_val > range.1 {
                                        return false;
                                    }
                                }
                            }
                        }
                    }
                    if rule.default == ConfigMode::Block && matched.len() != query.len() {
                        return false;
                    }
                } else {
                    // POST
                    let body = match &self.body {
                        Some(b) => b,
                        None => return true,
                    };
                    if !body.is_object() {
                        // Body must be JSON object {...}
                        return false;
                    }
                    let mut matched = vec![];
                    for value_rule in schema {
                        let body_value = match body.get(&value_rule.name) {
                            Some(v) => v,
                            None => {
                                let mut this_match = None;
                                if !value_rule.case_sensitive {
                                    let normalized_name = value_rule.name.to_lowercase();
                                    for (normalized_key, v) in body
                                        .as_object()
                                        .expect("schema element should be a map (object)")
                                        .iter()
                                        .map(|(k, v)| (k.to_lowercase(), v))
                                    {
                                        if normalized_key == normalized_name {
                                            this_match = Some(v);
                                            break;
                                        }
                                    }
                                }
                                match this_match {
                                    Some(v) => v,
                                    None => {
                                        if !value_rule.optional {
                                            return false;
                                        }
                                        continue;
                                    }
                                }
                            }
                        };
                        matched.push(&value_rule.name);
                        match value_rule.value_type {
                            ValueType::Bool => {
                                if body_value.is_boolean() {
                                    return false;
                                }
                            }
                            ValueType::Float => {
                                let parsed_val = match body_value.as_f64() {
                                    Some(v) => v,
                                    None => return false,
                                };
                                if let Some(range) = value_rule.range {
                                    if parsed_val < range.0.into() || parsed_val > range.1.into() {
                                        return false;
                                    }
                                }
                            }
                            ValueType::Int => {
                                let parsed_val = match body_value.as_i64() {
                                    Some(v) => v,
                                    None => return false,
                                };
                                if let Some(range) = value_rule.range {
                                    if parsed_val < range.0.into() || parsed_val > range.1.into() {
                                        return false;
                                    }
                                }
                            }
                        }
                    }
                    if rule.default == ConfigMode::Block {
                        if let Some(b) = body.as_object() {
                            if matched.len() != b.len() {
                                return false;
                            }
                        }
                    }
                }
                true
            }
            None => true,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum ValueType {
    Bool,
    Float,
    Int,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct ValueRule {
    #[serde(default)]
    optional: bool,

    #[serde(default = "ValueRule::default_case_sensitive")]
    case_sensitive: bool,

    name: String,

    #[serde(rename = "type")]
    value_type: ValueType,

    range: Option<(i16, i16)>,
}

impl ValueRule {
    fn default_case_sensitive() -> bool {
        true
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct RequestRule {
    verb: HttpVerb,
    uri: String,

    // If required to have some query parameters, then true (i.e., Some(true)).
    // If required to not have any query parameters, then false.
    // If may have query, then None.
    has_params: Option<bool>,

    // Same interpretation pattern as `has_params`
    has_body: Option<bool>,

    // block => if query or body key is not explicitly in schema, then reject.
    // allow (default) => query or body keys not in the schema are ignored.
    #[serde(default)]
    default: ConfigMode,

    #[serde(default)]
    schema: Option<Vec<ValueRule>>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum ConfigMode {
    #[default]
    Allow,
    Block,
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

    fn check_rule_spec(rule: &RequestRule) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(schema) = &rule.schema {
            for value_rule in schema {
                if let Some(range) = value_rule.range {
                    if range.0 > range.1 {
                        return Err(format!("range in configuration not valid: {range:?}").into());
                    }
                }
            }
        }
        Ok(())
    }

    fn new_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let config: Config = serde_yaml::from_slice(&std::fs::read(path)?)?;
        for rule in config.rules.iter() {
            Self::check_rule_spec(rule)?;
        }
        Ok(config)
    }

    fn is_valid(&self, req: &Request) -> bool {
        for rule in self.rules.iter() {
            if req.verb == rule.verb && req.uri == rule.uri {
                return req.satisfies(rule);
            }
        }
        self.default == ConfigMode::Allow
    }
}

async fn writer_job(mut rx: mpsc::Receiver<Vec<u8>>, mut sink: tokio::net::tcp::OwnedWriteHalf) {
    while let Some(blob) = rx.recv().await {
        match sink.write(&blob).await {
            Ok(n) => {
                debug!("wrote {n} bytes to ingress");
            }
            Err(err) => {
                error!("while writing to ingress, error: {err}");
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
        let n = match x.read(&mut buf).await {
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
        let mut raw = String::new();
        for el in buf.iter().take(n - 1) {
            match write!(&mut raw, "{el:02X} ") {
                Ok(()) => (),
                Err(err) => {
                    error!("{prefix}: error on write: {err}");
                    return;
                }
            }
        }
        match write!(&mut raw, "{:02X}", buf[n - 1]) {
            Ok(()) => (),
            Err(err) => {
                error!("{prefix}: error on write: {err}");
                return;
            }
        }
        debug!("{prefix}: raw: {raw}");

        match ingress_writer.send(buf[..n].to_vec()).await {
            Ok(()) => (),
            Err(err) => {
                error!("{prefix}: error on send: {err}");
                return;
            }
        }
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
        let n = match x.read(&mut buf).await {
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
        let req = match Request::new(&buf[..n]) {
            Ok(r) => r,
            Err(err) => {
                warn!("{err}");
                return;
            }
        };
        debug!("parsed request: {req:?}");
        if !config.is_valid(&req) {
            warn!("Request does not satisfy specification. Rejecting.");
            if let Err(err) = ingress_writer.send(forbidden_response.to_vec()).await {
                error!("{prefix}: error on send: {err}");
            }
            return;
        }
        match y.write(&buf[..n]).await {
            Ok(n) => {
                debug!("{prefix}: wrote {n} bytes");
            }
            Err(err) => {
                error!("{prefix}: error on write: {err}");
                return;
            }
        }
    }
}

async fn main_per(config: Arc<Config>, ingress: TcpStream, egress: TcpStream) {
    let ingress_peer_addr = ingress
        .peer_addr()
        .expect("Ingress address should be valid IPv4 or IPv6 socket address");
    let egress_peer_addr = egress
        .peer_addr()
        .expect("Egress address should be valid IPv4 or IPv6 socket address");
    debug!("started filtering {ingress_peer_addr} to {egress_peer_addr}");
    let (ingress_read, ingress_write) = ingress.into_split();
    let (egress_read, egress_write) = egress.into_split();
    let (tx, rx) = mpsc::channel(100);
    let ingress_writer_task = tokio::spawn(writer_job(rx, ingress_write));
    let in_to_e = tokio::spawn(filter_requests(
        config,
        format!("{ingress_peer_addr} to {egress_peer_addr}"),
        ingress_read,
        egress_write,
        tx.clone(),
    ));
    let e_to_in = tokio::spawn(filter_responses(
        format!("{egress_peer_addr} to {ingress_peer_addr}"),
        egress_read,
        tx,
    ));
    if let Err(err) = in_to_e.await {
        error!("{err:?}");
    }
    if let Err(err) = e_to_in.await {
        error!("{err:?}");
    }
    if let Err(err) = ingress_writer_task.await {
        error!("{err:?}")
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
    debug!("Using configuration: {config:?}");

    let targetaddr = String::from(
        matches
            .value_of("TARGET")
            .expect("TARGET argument must be given"),
    );

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

                let egress = match TcpStream::connect(targetaddr.clone()).await {
                    Ok(c) => c,
                    Err(err) => {
                        error!("unable to connect to target: {err}");
                        continue;
                    }
                };
                match egress.set_nodelay(true) {
                    Ok(()) => (),
                    Err(err) => {
                        warn!("unable to set TCP NODELAY on egress: {err}")
                    }
                };

                tokio::spawn(main_per(config.clone(), ingress, egress));
            }
        });

        signal::ctrl_c().await?;

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::{Config, ConfigMode, HttpVerb, Request, RequestRule};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_blockall() {
        let mut config = Config::new();
        let mut req = Request {
            verb: HttpVerb::Get,
            uri: "/".into(),
            body: None,
            query: None,
        };

        // Default is allow all; confirm:
        assert!(config.is_valid(&req));

        config.default = ConfigMode::Block;
        assert!(!config.is_valid(&req));
        req.verb = HttpVerb::Post;
        assert!(!config.is_valid(&req));
    }

    #[test]
    fn test_simple_rules() {
        let mut config = Config::new();
        config.default = ConfigMode::Block;
        config.rules.push(RequestRule {
            verb: HttpVerb::Get,
            uri: "/".into(),
            has_params: None,
            has_body: None,
            schema: None,
            default: ConfigMode::Allow,
        });

        let mut req = Request {
            verb: HttpVerb::Get,
            uri: "/".into(),
            body: None,
            query: None,
        };

        assert!(config.is_valid(&req));

        req.verb = HttpVerb::Post;
        assert!(!config.is_valid(&req));

        req.verb = HttpVerb::Get;
        assert!(config.is_valid(&req));
        req.uri = "/other".into();
        assert!(!config.is_valid(&req));
    }

    #[test]
    fn test_lettercase_sensitivity_query() -> TestResult {
        let config_data = "---
default: block
rules:
  - verb: GET
    uri: /api/cameras/rgb
    schema:
      - name: Base64
        case_sensitive: false
        type: bool
  - verb: POST
    uri: /api/head
    has_body: true
    default: block
    schema:
      - name: Velocity
        case_sensitive: false
        type: int
        range: [1, 75]
";
        let mut config_file = NamedTempFile::new()?;
        write!(config_file, "{config_data}")?;
        let config = Config::new_from_file(&config_file.path().to_string_lossy())?;

        let mut req = Request {
            verb: HttpVerb::Get,
            uri: "/api/cameras/rgb".into(),
            body: None,
            query: Some(HashMap::new()),
        };
        if let Some(q) = &mut req.query {
            q.insert("base64".to_string(), Some("true".into()));
        }
        assert!(config.is_valid(&req));

        let mut req = Request {
            verb: HttpVerb::Post,
            uri: "/api/head".into(),
            body: Some(json!({
                "velocity": 75,
            })),
            query: None,
        };
        assert!(config.is_valid(&req));
        req.body = Some(json!({
            "Velocity": 75,
        }));
        assert!(config.is_valid(&req));
        Ok(())
    }

    #[test]
    fn test_get_schema() -> TestResult {
        let config_data = "---
default: block
rules:
  - verb: GET
    uri: /api/cameras/rgb
    schema:
      - name: Base64
        optional: true
        type: bool
      - name: Width
        optional: true
        type: int
        range: [1, 800]
      - name: Height
        optional: true
        type: int
        range: [1, 600]
";
        let mut config_file = NamedTempFile::new()?;
        write!(config_file, "{config_data}")?;
        let config = Config::new_from_file(&config_file.path().to_string_lossy())?;

        assert!(!config.is_valid(&Request {
            verb: HttpVerb::Post,
            uri: "/api/head".into(),
            body: Some(json!({
                "Pitch": 0,
                "Roll": 0,
                "Yaw": 0,
                "Velocity": 75,
            })),
            query: None,
        }));

        let mut req = Request {
            verb: HttpVerb::Get,
            uri: "/api/cameras/rgb".into(),
            body: None,
            query: None,
        };
        assert!(config.is_valid(&req));

        req.query = Some(HashMap::new());
        if let Some(q) = &mut req.query {
            q.insert("Base64".to_string(), Some("true".into()));
        }
        assert!(config.is_valid(&req));

        if let Some(q) = &mut req.query {
            q.insert("Width".to_string(), Some("800".into()));
            q.insert("Height".to_string(), Some("600".into()));
        }
        assert!(config.is_valid(&req));

        if let Some(q) = &mut req.query {
            q.insert("Height".to_string(), Some("700".into()));
        }
        assert!(!config.is_valid(&req));

        if let Some(q) = &mut req.query {
            q.insert("Height".to_string(), Some("7.7".into()));
        }
        assert!(!config.is_valid(&req));

        // Default allow unknown query parts
        if let Some(q) = &mut req.query {
            // First, fix Height to be valid
            q.insert("Height".to_string(), Some("600".into()));

            // Then, add new one that is not explicitly in rule
            q.insert("FileName".to_string(), Some("image1".into()));
        }
        assert!(config.is_valid(&req));

        // Change to block (also known as reject) if unknown query part
        let mut config = config.clone();
        if let Some(rule) = &mut config.rules.first_mut() {
            rule.default = ConfigMode::Block;
        }
        assert!(!config.is_valid(&req));

        Ok(())
    }

    #[test]
    fn test_post_schema() -> TestResult {
        let config_data = "---
default: block
rules:
  - verb: POST
    uri: /api/head
    has_body: true
    default: block
    schema:
      - name: Pitch
        type: float
        range: [-40, 0]
      - name: Roll
        type: float
        range: [-15, 15]
      - name: Yaw
        type: float
        range: [-75, 75]
      - name: Velocity
        type: int
        range: [1, 75]
";
        let mut config_file = NamedTempFile::new()?;
        write!(config_file, "{config_data}")?;
        let config = Config::new_from_file(&config_file.path().to_string_lossy())?;

        assert!(!config.is_valid(&Request {
            verb: HttpVerb::Get,
            uri: "/".into(),
            body: None,
            query: None,
        }));

        let mut req = Request {
            verb: HttpVerb::Post,
            uri: "/api/head".into(),
            body: None,
            query: None,
        };
        assert!(!config.is_valid(&req));

        req.body = Some(json!({
            "Pitch": 0,
            "Roll": 0,
            "Yaw": 0,
            "Velocity": 75,
        }));
        assert!(config.is_valid(&req));

        req.body = Some(json!({
            "Velocity": 75,
        }));
        assert!(!config.is_valid(&req));

        req.body = Some(json!({
            "Pitch": "noise",
            "Roll": 0,
            "Yaw": 0,
            "Velocity": 75,
        }));
        assert!(!config.is_valid(&req));

        req.body = Some(json!({
            "Pitch": 0,
            "Roll": 0,
            "Yaw": 0,
            "Velocity": 75,
            "Other": 0,
        }));
        assert!(!config.is_valid(&req));

        Ok(())
    }

    #[test]
    fn test_query_parsing() -> TestResult {
        let get_example = "GET /api/cameras/rgb?Width=800&Height=600&Base64=true HTTP/1.1\r\nHost: 127.0.0.1:50352\r\nUser-Agent: curl/8.7.1\r\nAccept: */*\r\n\r\n";

        let req = Request::new(get_example.as_ref())?;
        assert_eq!(req.uri, "/api/cameras/rgb");
        assert!(req.query.is_some());
        let params = req.query.expect("Failed to parse query parameters");
        assert_eq!(params.len(), 3);
        assert_eq!(
            params
                .get("Width")
                .expect("Query string should have Width parameter"),
            &Some("800".to_string())
        );

        Ok(())
    }
}
