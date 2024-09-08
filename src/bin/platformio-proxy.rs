// Copyright (C) 2023 rerobots, Inc.

use std::env;
use std::io::prelude::*;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process;

use serde::{Deserialize, Serialize};


enum Mode {
    Server,
    Client,
}


#[derive(Serialize, Deserialize, Debug)]
struct Build {
    platformio_ini: String,
    blob: String,
    blob_path: String,
}


fn serv(addr: std::net::SocketAddr, build_file: PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let blob = std::fs::read_to_string(build_file)?;
    Ok(())
}

fn client(addr: std::net::SocketAddr) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(addr)?;
    let mut raw_read: Vec<u8> = vec![];
    let raw_read_count = stream.read_to_end(&mut raw_read)?;
    let b: Build = serde_json::from_slice(&raw_read)?;
    Ok(())
}


fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args_os().len() != 3 && env::args_os().len() != 4 {
        println!("Usage: platformio-proxy MODE ADDR [FILE]");
        process::exit(1);
    }
    let mode = env::args_os().nth(1).unwrap();
    let mode = if mode == "s" {
        Mode::Server
    } else if mode == "c" {
        Mode::Client
    } else {
        println!("unknown mode: {:?}", mode);
        process::exit(1);
    };

    let addr: std::net::SocketAddr = env::args_os().nth(2).unwrap().to_str().unwrap().parse()?;

    match mode {
        Mode::Server => {
            if env::args_os().len() < 4 {
                println!("Error: FILE required in server mode");
                process::exit(1);
            }
            let build_file = PathBuf::from(env::args_os().nth(3).unwrap());
            serv(addr, build_file)
        }
        Mode::Client => client(addr),
    }
}
