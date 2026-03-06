use napi_derive::napi;

#[napi]
pub fn version() -> String {
    rhei_core::version()
}

#[napi]
pub fn help() -> String {
    rhei_core::help_text()
}
