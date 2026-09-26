# Independent review prompt and output

Review the integrated task diff against its task contract and root AGENTS.md.
Remain read-only unless assigned a separate test-writing task. Inspect actual
code and logs, not only the author's summary. Do not publish or launch agents.

Check applicable behavior:

- Identity, per-site state, repeated tests, latest-attempt ordering and unknowns.
- Raw record provenance, null/default handling, timestamp interpretation.
- Schema/cache compatibility, atomic publication, quotas and cancellation.
- Numerical definitions, denominator correctness, axes, units and legends.
- Live/offline parity, theme/language coverage and user-visible drilldown.
- Ownership compliance, unrelated changes and accidental third-party data.
- Whether evidence actually covers the changed behavior; missing evidence is
  an explicit verification gap, not an assumed pass.

Return:

1. Reviewed task and revision/diff scope.
2. Findings: severity, exact file/line, reproducible scenario, impact and fix.
3. Verification performed and remaining gaps.
4. Recommendation: ready, changes requested, or blocked with reason.

If no defects are found, say so and still state the review's coverage limits.
