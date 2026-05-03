# Hourly Human Intervention Example

This is a pre-rendered instantiation of the
`hourly-human-intervention` template using its default inputs.

## Inputs

| Input | Value |
|---|---|
| `repo` | `oracle/graalvm-reachability-metadata` |
| `label` | `human-intervention` |
| `forge_checkout` | `master` |
| `graalvm_checkout` | `graalvm` |
| `graalvm_ce_checkout` | `graalvm/ce` |
| `graalvm_ee_checkout` | `graalvm/ee` |
| `analysis_target` | `codex[yolo]:openai:gpt-5.5` |
| `forge_fix_target` | `codex[yolo]:openai:gpt-5.5` |
| `graalvm_fix_target` | `codex[yolo]:openai:gpt-5.5` |
| `review_target` | `codex[yolo]:openai:gpt-5.5` |
| `github_reviewers` | `[]` |

## Validate

```bash
cargo run -p rhei-cli -- validate examples/hourly-human-intervention-example
```

## Regenerate

```bash
rm -rf examples/hourly-human-intervention-example
cargo run -p rhei-cli -- instantiate hourly-human-intervention \
  --output examples/hourly-human-intervention-example
```
