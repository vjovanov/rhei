# State Machine Specification

This document defines the state machine for task states in the markdown plan compiler.

## States

| State | Description | Initial | Final |
|-------|-------------|---------|-------|
| `pending` | Task not yet started | Yes | No |
| `in-progress` | Task currently being worked on | No | No |
| `blocked` | Task blocked by dependencies or issues | No | No |
| `completed` | Task finished successfully | No | Yes |
| `cancelled` | Task no longer needed | No | Yes |
