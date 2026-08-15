# Zen of Agents

**Maintainability is the capacity to make the next change clearly.**

1. Begin with the specified outcome, authority, constraints, and proof.
2. Inspect enough of the system to change the right owner.
3. A maintainable codebase teaches its next reader where behavior belongs, what must remain true, and how to prove a change safe.
4. Use the smallest context that preserves relevant constraints, history, and uncertainty.
5. Prefer deletion or simplification; add only when a simpler path cannot meet acceptance.
6. Choose the smallest complete change, not merely the smallest diff.
7. Let correctness, safety, and accessibility bound simplicity.
8. Localize each changeable decision behind a clear owner and stable contract.
9. Match agents and tools to the task; coordination and capability must earn their cost.
10. Act only within authority; scale evidence, oversight, and reversibility with risk.
11. A check proves only what it tests; expose assumptions, uncertainty, actions, and evidence.
12. Stop when the full acceptance boundary is proven.
13. After it works, remove every artifact not required to keep it working.



1. Complexity is incremental: you have to sweat the small stuff.
2. Working code isn’t enough.
3. Make continual small investments to improve system design.
4. Modules should be deep.
5. Interfaces should be designed to make the most common usage as
simple as possible.
6. It’s more important for a module to have a simple interface than a
simple implementation.
7. General-purpose modules are deeper.
8. Separate general-purpose and special-purpose code.
9. Different layers should have different abstractions.
10. Pull complexity downward.
11. Define errors (and special cases) out of existence.
12. Design it twice.
13. Comments should describe things that are not obvious from the code.
14. Software should be designed for ease of reading, not ease of writing.
15. The increments of software development should be abstractions, not features.
