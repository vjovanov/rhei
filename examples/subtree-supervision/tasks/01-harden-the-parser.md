### Task 1: Harden the parser
**State:** supervise

Goal and acceptance criteria for the whole change: the parser must reject
malformed input without panicking, and the fixes must be the ones the review
asked for — no more.

Judge every checkpoint, brief the next step, and write the summary that ties
the four children together once they are all done.

#### Task 1.1: Review parser
**State:** review

Read the parser and write findings.

#### Task 1.2: Fix findings
**State:** fix
**Prior:** Task 1.1

Apply exactly the fixes the supervisor's brief asks for.

#### Task 1.3: Re-review
**State:** review
**Prior:** Task 1.2

Re-read the parser after the first fix pass.

#### Task 1.4: Fix remaining
**State:** fix
**Prior:** Task 1.3

Apply whatever the second review turned up. The supervisor cancels this step
when the re-review found nothing.
