# Default Worker Pass

Use this sequence when the plan uses the built-in `rhei` state machine:

```bash
rhei validate <plan>
rhei next <plan>
rhei transition <plan> --task <id> --from draft --to pending
# implement the task in pending
rhei transition <plan> --task <id> --from pending --to agent-review
# review or fix as instructed by the current state
rhei complete <plan> --task <id> --result "Implemented and verified"
```

If `rhei next` reports no claimable task, re-render or inspect the plan before changing anything by hand:

```bash
rhei render <plan> --format progress --no-color
```
