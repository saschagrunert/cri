//! Logging faccade for the FFI

use crate::ffi::error::update_last_err_if_required;
use anyhow::{bail, Context, Result};
use clap::crate_name;
use libc::c_char;
use std::{env, ffi::CStr};

#[no_mangle]
/// Init the log level by the provided level string.
/// Populates the last error on any failure.
pub extern "C" fn log_init(level: *const c_char) {
    update_last_err_if_required(log_init_res(level))
}

fn log_init_res(level: *const c_char) -> Result<()> {
    if level.is_null() {
        bail!("provided log level is NULL")
    }
    let log_level = unsafe { CStr::from_ptr(level) }
        .to_str()
        .context("convert log level string")?;
    env::set_var("RUST_LOG", format!("{}={}", crate_name!(), log_level));
    env_logger::try_init().context("init log level")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ffi::error::last_error_length;
    use std::ptr;

    #[test]
    fn log_init_success() {
        log_init("error\0".as_ptr() as *const c_char);
        assert_eq!(last_error_length(), 0);
    }

    #[test]
    fn log_init_failure_level_null() {
        log_init(ptr::null() as *const c_char);
        assert!(last_error_length() > 0);
    }
}
