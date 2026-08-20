// Primary entry point. All CLI logic lives in the library target so the `rh`
// alias binary links the same compiled code instead of rebuilding it.
// §FS-rhei-distribution.1
fn main() {
    rhei_cli::run();
}
