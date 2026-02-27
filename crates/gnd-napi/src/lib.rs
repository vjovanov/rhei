use napi_derive::napi;

#[napi]
pub fn version() -> String {
    gnd_core::version()
}

#[napi]
pub fn help() -> String {
    gnd_core::help_text()
}
