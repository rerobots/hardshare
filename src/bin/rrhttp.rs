// Copyright (C) 2024 rerobots, Inc.

#[macro_use]
extern crate clap;
use clap::Arg;

#[macro_use]
extern crate log;

use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Builder;


fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let matches = clap::App::new("rrhttp")
        .arg(
            Arg::with_name("TARGET")
                .required(true)
                .help("target HOST:PORT"),
        )
        .version(crate_version!())
        .get_matches();

    let targetaddr = String::from(matches.value_of("TARGET").unwrap());

    let rt = Builder::new_current_thread().enable_io().build()?;
    rt.block_on(async {
        let mut listener = TcpListener::bind("127.0.0.1:0").await?;
        println!("{}", listener.local_addr()?);
        Ok(())
    })
}
