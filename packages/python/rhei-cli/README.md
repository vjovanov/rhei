# rhei-cli

Python package for the `rhei` command.

```bash
pip install rhei-cli
rhei version
rhei lsp
```

This alpha package installs the Rust `rhei-cli` crate with Cargo on first use.
Rust and Cargo must be available on `PATH`.

LSP-capable editors can launch `rhei lsp` over stdio for `.rhei.md`
diagnostics, completions, symbols, hover, and same-document definitions.
