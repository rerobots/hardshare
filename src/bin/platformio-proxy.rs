// Copyright (C) 2023 rerobots, Inc.
//
// platformio-proxy safely moves firmware from inside containers to attached devices.
//
//     upload_protocol = custom
//     upload_command = platformio-proxy $PROJECT_CONFIG $SOURCE

use std::env;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process;

#[macro_use]
extern crate log;

use serde::{Deserialize, Serialize};

enum Mode {
    Server,
    Client,
}

#[derive(Serialize, Deserialize, Debug)]
struct Build {
    platformio_ini: Vec<u8>,
    blob: Vec<u8>,
}

fn serv(addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let header_len = 9;
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
        let ini_size = u8vec_to_usize(&raw_read[1..5]);
        let exe_size = u8vec_to_usize(&raw_read[5..9]);
        let b = Build {
            platformio_ini: raw_read
                .drain(header_len..(header_len + ini_size))
                .collect(),
            blob: raw_read
                .drain(header_len..(header_len + exe_size))
                .collect(),
        };
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

    let mut header: Vec<u8> = vec![0];
    header.append(&mut usize_to_u8vec(ini_data.len())?);
    header.append(&mut usize_to_u8vec(exe_data.len())?);

    stream.write_all(&header)?;
    stream.write_all(&ini_data)?;
    stream.write_all(&exe_data)?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args_os().len() != 3 && env::args_os().len() != 5 {
        eprintln!("Usage: platformio-proxy MODE ADDR [INI EXE]");
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

    match mode {
        Mode::Client => {
            let ini_file = match env::args_os().nth(3) {
                Some(p) => PathBuf::from(p),
                None => {
                    eprintln!("Error: platformio.ini required in client mode");
                    process::exit(1);
                }
            };
            let exe_file = match env::args_os().nth(4) {
                Some(p) => PathBuf::from(p),
                None => {
                    eprintln!("Error: built executable file required in client mode");
                    process::exit(1);
                }
            };
            client(addr, ini_file, exe_file)
        }
        Mode::Server => serv(addr),
    }
}
