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
    #[cfg(unix)]
    {
        openssl_probe::init_ssl_cert_env_vars();
    }
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
