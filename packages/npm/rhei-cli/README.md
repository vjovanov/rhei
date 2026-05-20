# rhei

npm package for the `rhei` command.

```bash
npm install -g rhei
rhei version
rhei lsp
```

This alpha package installs the Rust `rhei-cli` crate with Cargo during
`postinstall`. Rust and Cargo must be available on `PATH`.

LSP-capable editors can launch `rhei lsp` over stdio for `.rhei.md`
diagnostics, completions, symbols, hover, and same-document definitions.
