// Copyright (C) 2026 rerobots, Inc.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;
use std::time::Duration;

#[macro_use]
extern crate clap;
use clap::Arg;

#[macro_use]
extern crate log;

fn passthru(mut stream: TcpStream, dev: &str) -> Result<(), Box<dyn std::error::Error>> {
    debug!("start passthru()");
    stream.set_nodelay(true)?;

    let mut sp = serialport::new(dev, 115_200)
        .timeout(Duration::from_millis(1000))
        .open()?;

    let mut sp_cp = sp.try_clone()?;
    let mut stream_cp = stream.try_clone()?;

    let egress = thread::spawn(move || {
        let mut buf: Vec<u8> = vec![0; 128];
        loop {
            let nb = match sp_cp.read(&mut buf) {
                Ok(nb) => nb,
                Err(err) => {
                    warn!("serial read: {err}");
                    return;
                }
            };
            info!("read {nb} bytes via serial");
            if let Err(err) = stream_cp.write_all(&buf[..nb]) {
                warn!("TCP write: {err}");
                return;
            }
        }
    });

    let mut buf: Vec<u8> = vec![0; 128];
    loop {
        let nb = match stream.read(&mut buf) {
            Ok(nb) => nb,
            Err(err) => {
                warn!("TCP read: {err}");
                break;
            }
        };
        info!("read {nb} bytes via TCP");
        if nb == 0 {
            break;
        }
        if let Err(err) = sp.write_all(&buf[..nb]) {
            warn!("serial write: {err}");
            break;
        }
    }
    if let Err(err) = egress.join() {
        error!("{err:?}");
    }

    debug!("done passthru()");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let matches = clap::App::new("rrserial")
        .max_term_width(80)
        .arg(
            Arg::with_name("address")
                .long("address")
                .short("a")
                .value_name("ADDR")
                .help("Accept connections at this address. Default is 127.0.0.1:0, i.e., listen on localhost (127.0.0.1) at an automatically assigned port."),
        )
        .arg(
            Arg::with_name("DEVICE")
                .required(true)
                .help("serial device"),
        )
        .version(crate_version!())
        .get_matches();

    let dev = matches
        .value_of("DEVICE")
        .expect("DEVICE argument must be given");
    let addr = matches.value_of("address").unwrap_or("127.0.0.1:0");

    let listener = TcpListener::bind(addr)?;
    println!("{}", listener.local_addr()?);

    for stream in listener.incoming() {
        passthru(stream?, dev)?;
    }

    Ok(())
}
