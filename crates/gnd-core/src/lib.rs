pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn help_text() -> String {
    "GND (ground) - Markdown plan compiler scaffold\n\nUsage:\n  gnd [OPTIONS]\n\nFor now, use --help and --version."
        .to_string()
}
