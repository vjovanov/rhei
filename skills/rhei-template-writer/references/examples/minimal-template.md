# Minimal Template Shape

```text
feature-rollout/
+-- template.yaml
+-- README.md
`-- plan.rhei.md
```

`template.yaml`:

```yaml
name: feature-rollout
version: 1
description: Plan a small feature rollout.
inputs:
  - name: feature_name
    description: Feature or capability being rolled out.
    type: string
```

`plan.rhei.md`:

```markdown
# Rhei: Roll Out {{ feature_name }}

## Tasks

### Task 1: Prepare rollout
**State:** draft

Confirm scope, owners, and verification for {{ feature_name }}.
```
