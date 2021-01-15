// SCL <scott@rerobots.net>
// Copyright (C) 2021 rerobots, Inc.

#[macro_use]
extern crate clap;

mod api;
mod cli;
mod mgmt;


fn main() {
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
