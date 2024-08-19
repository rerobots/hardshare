use std::process::Command;
use std::thread::sleep;

use crate::api;
use crate::check::Error;
use crate::mgmt::Config;


fn run_opt(
    local_config: &Config,
    wd_index: usize,
    handle_errors: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(prog) = &local_config.wdeployments[wd_index].monitor {
        let result = match Command::new("/bin/sh").args(["-c", prog.as_str()]).status() {
            Ok(result) => {
                if !result.success() {
                    let msg = format!("monitor: `{prog}` failed: {result}");
                    warn!("{}", msg);
                    Err(Error::new(msg))
                } else {
                    Ok(())
                }
            }
            Err(err) => {
                let msg = format!("monitor: `{prog}` failed: {err}");
                warn!("{}", msg);
                Err(Error::new(msg))
            }
        };

        if result.is_err() {
            if handle_errors {
                let ac = api::HSAPIClient::new();
                ac.toggle_lockout(&local_config.wdeployments[wd_index].id, true)?;
            }
            return Ok(result?);
        }
    }
    Ok(())
}


pub fn run_dry(local_config: &Config, wd_index: usize) -> Result<(), Box<dyn std::error::Error>> {
    run_opt(local_config, wd_index, false)
}


pub fn run(local_config: &Config, wd_index: usize) -> Result<(), Box<dyn std::error::Error>> {
    run_opt(local_config, wd_index, true)
}


pub fn run_loop(
    local_config: &Config,
    wd_index: usize,
    duration: std::time::Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        run(local_config, wd_index)?;
        sleep(duration);
    }
}
