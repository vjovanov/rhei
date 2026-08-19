# FS-rhei-version: `rhei version`

Print the CLI and core crate versions so operators can include exact tool
versions in bug reports, CI logs, release checks, and support handoffs. §GOAL-rhei-outcomes

## 1. Usage

```bash
rhei version
```

The command takes no arguments or options.

## 2. Behavior

`rhei version` prints one line per surfaced component:

```text
rhei-cli <version>
rhei-core <version>
rhei-validator <version>
rhei-output <version>
```

The version values come from the compiled crate metadata. `rhei-validator` and
`rhei-output` are modules inside `rhei-cli` rather than separate packages
(§FS-rhei-distribution.1); they stay on their own lines because §FS-rhei-version.3
promises the component names are stable for scripts. The command does not read
plans, load state machines, inspect settings, touch runtime files, or perform
network access.

## 3. Output Contract

The output is plain text on stdout. Each line is `<component> <semver-or-build-version>`.
The component names are stable for scripts that need to extract a specific
component version, whether or not that component is a separately published crate.

## Related Specifications

- [Validate Command](rhei-validate.spec.md) - command used to verify plans
- [States Command](rhei-states-cmd.spec.md) - command used to inspect state-machine configuration
