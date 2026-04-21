# Rhei: {{plan_title}}
**States:** multi-model-analysis

## Overview
This plan runs the same analytical prompt through multiple model-specific
analysis tasks. Once all model analyses are complete, `claude` produces the
final synthesis document.

## Tasks

### Task claude-analysis: Claude analysis — {{task_title}}
**State:** claude-analyze

{{task_description}}

Write the Claude analysis note to `{{analysis_output_dir}}/claude.md`.

### Task gemini-analysis: Gemini analysis — {{task_title}}
**State:** gemini-analyze

{{task_description}}

Write the Gemini analysis note to `{{analysis_output_dir}}/gemini.md`.

### Task codex-analysis: Codex analysis — {{task_title}}
**State:** codex-analyze

{{task_description}}

Write the Codex analysis note to `{{analysis_output_dir}}/codex.md`.

### Task summary: Claude synthesis — {{task_title}}
**State:** summarize
**Prior:** Task claude-analysis, Task gemini-analysis, Task codex-analysis

Read the model analysis notes and write the final synthesized document to
`{{final_document_path}}`.
