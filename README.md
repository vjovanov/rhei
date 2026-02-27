# GND (ground) - CLI scaffold

Rust-based CLI tool scaffold for "GND" with a multi-crate Cargo workspace:
- gnd-core: core library (version/help surface)
- gnd-cli: binary CLI (clap-based) named "gnd"
- gnd-napi: N-API bindings exposing version/help for Node.js

This is Task gnd-2qz.1 of the "GND CLI tool scaffold and distribution" epic.

Notes:
- Basic CLI wiring and richer features, npm packaging, and CI/CD will be added in subsequent tasks.
- This initial scaffold builds a workspace and minimal surfaces to unblock further work.
