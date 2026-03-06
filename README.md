# Rhei - CLI scaffold

Rust-based CLI tool scaffold for "Rhei" with a multi-crate Cargo workspace:
- rhei-core: core library (version/help surface)
- rhei-cli: binary CLI (clap-based) named "rhei"
- rhei-napi: N-API bindings exposing version/help for Node.js

Notes:
- Basic CLI wiring and richer features, npm packaging, and CI/CD will be added in subsequent tasks.
- This initial scaffold builds a workspace and minimal surfaces to unblock further work.
