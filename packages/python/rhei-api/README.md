# rhei-api

Python helper API for Rhei.

```python
from rhei_api import version, run

print(version())
result = run(["validate", "plan.rhei.md"], capture_output=True)
```

This alpha package depends on `rhei-cli`, which installs the Rust CLI with
Cargo on first use.
