// The N-API shims resolve their symbols against the host Node process, and a
// test harness is not one. `test = false` in the manifest is not enough: a run
// that selects the lib target explicitly — `--all-targets` does — overrides it,
// and the harness that results loads no Node and aborts on Windows before it
// can report its zero tests. Compiling the crate away under `cfg(test)` leaves
// that harness with nothing to register. #91
#![cfg(not(test))]

use napi_derive::napi;

#[napi]
pub fn version() -> String {
    rhei_core::version()
}

#[napi]
pub fn help() -> String {
    rhei_core::help_text()
}
