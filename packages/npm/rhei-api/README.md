# rhei-api

JavaScript helper API for Rhei.

```js
const { version, runCaptureSync } = require("rhei-api");

console.log(version());
const result = runCaptureSync(["validate", "plan.rhei.md"]);
```

This alpha package depends on the `rhei` npm package, which installs the Rust CLI with
Cargo during npm installation.
