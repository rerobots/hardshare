// SCL <scott@rerobots.net>
// Copyright (C) 2021 rerobots, Inc.

#[macro_use]
extern crate log;

#[macro_use]
extern crate clap;

#[macro_use]
extern crate serde_json;

mod api;
mod cli;
mod mgmt;


fn main() {
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "WARN");
    }
    env_logger::init();

    match cli::main() {
        Ok(_) => std::process::exit(0),
        Err(err) => {
            if err.msg.is_some() {
                eprintln!("{}", err);
            }
            std::process::exit(err.exitcode);
        }
    }
}
