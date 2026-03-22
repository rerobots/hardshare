// Copyright (C) 2023 rerobots, Inc.
//
// platformio-proxy safely moves firmware from inside containers to attached devices.
//
//     upload_protocol = custom
//     upload_command = platformio-proxy $PROJECT_CONFIG $SOURCE

use std::convert::TryFrom;
use std::env;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process;
use std::process::{Command, Stdio};

use tempfile::NamedTempFile;

#[macro_use]
extern crate log;

use serde::{Deserialize, Serialize};

enum Mode {
    Server,
    Client,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
enum Runtime {
    Docker,
    Podman,
}

impl TryFrom<String> for Runtime {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "docker" => Ok(Self::Docker),
            "podman" => Ok(Self::Podman),
            _ => Err("runtime must be one of the following: docker, podman"),
        }
    }
}

impl std::fmt::Display for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Docker => write!(f, "docker"),
            Self::Podman => write!(f, "podman"),
        }
    }
}

const COMMAND_UPLOAD: u8 = 0;

#[derive(Serialize, Deserialize, Debug)]
struct Build {
    platformio_ini: Vec<u8>,
    blob: Vec<u8>,
}

fn do_upload(
    runtime: &Runtime,
    build: Build,
    host_platformio_ini: &str,
    build_path: &str,
    img: &str,
    device: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let host_ini_temp = NamedTempFile::new()?;
    let host_build_temp = NamedTempFile::new()?;

    std::fs::write(host_ini_temp.path(), host_platformio_ini)?;
    std::fs::write(host_build_temp.path(), build.blob)?;
    let mut proc = Command::new(runtime.to_string())
        .args([
            "run",
            "--rm",
            "-it",
            "-v",
            &format!("{}:/root/platformio.ini:ro", host_ini_temp.path().display()),
            "-v",
            &format!("{}:{}:ro", host_build_temp.path().display(), build_path),
            &format!("--device={device}:{device}"),
            img,
            "bash",
            "-ic",
            "cd $HOME && pio run -t nobuild -t upload",
        ])
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = proc
        .stdout
        .as_mut()
        .ok_or("stdout of upload process should be captured")?;
    let mut buf = vec![];
    let nb = stdout.read_to_end(&mut buf)?;
    let x = String::from_utf8_lossy(&buf[..nb]);
    println!("{}", x);
    Ok(())
}

fn serv(
    addr: std::net::SocketAddr,
    host_ini_file: PathBuf,
    runtime: Runtime,
    build_path: &str,
    img: &str,
    device: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let host_platformio_ini = String::from_utf8(std::fs::read(&host_ini_file)?)?;
    let listener = TcpListener::bind(addr)?;
    println!("{}", listener.local_addr()?);
    for stream_result in listener.incoming() {
        let mut stream = match stream_result {
            Ok(s) => s,
            Err(err) => {
                error!("{err:?}");
                continue;
            }
        };

        let mut header_len = 2;
        let mut raw_read: Vec<u8> = vec![];
        let raw_read_count = stream.read_to_end(&mut raw_read)?;
        if raw_read_count < header_len {
            warn!("Header is too small");
            continue;
        }
        let version = raw_read[0];
        if version != 0 {
            warn!("Unknown version: {version}");
            continue;
        }
        match raw_read[1] {
            COMMAND_UPLOAD => {
                header_len += 8;
                let ini_size = u8vec_to_usize(&raw_read[2..6]);
                let exe_size = u8vec_to_usize(&raw_read[6..10]);
                let b = Build {
                    platformio_ini: raw_read
                        .drain(header_len..(header_len + ini_size))
                        .collect(),
                    blob: raw_read
                        .drain(header_len..(header_len + exe_size))
                        .collect(),
                };
                do_upload(&runtime, b, &host_platformio_ini, build_path, img, device)?;
            }
            _ => {
                warn!("unknown command: {}", raw_read[1]);
                continue;
            }
        }
    }
    Ok(())
}

fn usize_to_u8vec(x: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Ignore bits beyond 32
    Ok(vec![
        (x & 0xff).try_into()?,
        ((x >> 8) & 0xff).try_into()?,
        ((x >> 16) & 0xff).try_into()?,
        ((x >> 24) & 0xff).try_into()?,
    ])
}

fn u8vec_to_usize(v: &[u8]) -> usize {
    let mut x: usize = 0;
    for (index, k) in v.iter().enumerate() {
        x |= (*k as usize) << (index * 8);
    }
    x
}

fn client(
    addr: std::net::SocketAddr,
    ini_file: PathBuf,
    exe_file: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let ini_data = std::fs::read(&ini_file)?;
    let exe_data = std::fs::read(&exe_file)?;
    let mut stream = TcpStream::connect(addr)?;

    let mut header: Vec<u8> = vec![0, COMMAND_UPLOAD];
    header.append(&mut usize_to_u8vec(ini_data.len())?);
    header.append(&mut usize_to_u8vec(exe_data.len())?);

    stream.write_all(&header)?;
    stream.write_all(&ini_data)?;
    stream.write_all(&exe_data)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args_os().len() != 5 && env::args_os().len() != 8 {
        eprintln!("Usage: platformio-proxy MODE ADDR INI [EXE] [RUNTIME BUILDPATH IMG DEV]");
        process::exit(1);
    }
    let mode = env::args_os().nth(1).expect("MODE argument is required");
    let mode = if mode == "s" {
        Mode::Server
    } else if mode == "c" {
        Mode::Client
    } else {
        eprintln!("unknown mode: {mode:?}");
        process::exit(1);
    };

    let addr: std::net::SocketAddr = env::args_os()
        .nth(2)
        .expect("ADDR argument is required")
        .to_str()
        .expect("ADDR should be valid unicode")
        .parse()?;

    let ini_file = match env::args_os().nth(3) {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("Error: platformio.ini required");
            process::exit(1);
        }
    };

    match mode {
        Mode::Client => {
            let exe_file = match env::args_os().nth(4) {
                Some(p) => PathBuf::from(p),
                None => {
                    eprintln!("Error: built executable file required in client mode");
                    process::exit(1);
                }
            };
            client(addr, ini_file, exe_file)
        }
        Mode::Server => {
            let arg = match env::args_os().nth(4) {
                Some(p) => match p.into_string() {
                    Ok(s) => s,
                    Err(err) => {
                        eprintln!("Error: unrecognized format of runtime: {err:?}");
                        process::exit(1);
                    }
                },
                None => {
                    eprintln!("Error: runtime required in server mode");
                    process::exit(1);
                }
            };
            let runtime = match Runtime::try_from(arg) {
                Ok(r) => r,
                Err(err) => {
                    eprintln!("Error: {err}");
                    process::exit(1);
                }
            };

            let build_path = match env::args_os().nth(5) {
                Some(p) => match p.into_string() {
                    Ok(s) => s,
                    Err(err) => {
                        eprintln!("Error: unrecognized format of build path: {err:?}");
                        process::exit(1);
                    }
                },
                None => {
                    eprintln!("Error: build path required in server mode");
                    process::exit(1);
                }
            };
            let img = match env::args_os().nth(6) {
                Some(p) => match p.into_string() {
                    Ok(s) => s,
                    Err(err) => {
                        eprintln!("Error: unrecognized format of image name: {err:?}");
                        process::exit(1);
                    }
                },
                None => {
                    eprintln!("Error: image name required in server mode");
                    process::exit(1);
                }
            };
            let device = match env::args_os().nth(7) {
                Some(p) => match p.into_string() {
                    Ok(s) => s,
                    Err(err) => {
                        eprintln!("Error: unrecognized format of device file name: {err:?}");
                        process::exit(1);
                    }
                },
                None => {
                    eprintln!("Error: device name required in server mode");
                    process::exit(1);
                }
            };
            serv(addr, ini_file, runtime, &build_path, &img, &device)
        }
    }
}
