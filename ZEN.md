# Zen of Agents

**Maintainability is the capacity to make the next change clearly.**

## Working agreements

These agreements are requirements. They define the boundaries of a change and
govern when a design heuristic conflicts with the specified outcome, authority,
constraints, or proof.

### Before changing

- Begin with the specified outcome, authority, constraints, and proof.
- Inspect enough of the system to change the right owner.
- Use the smallest context that preserves relevant constraints, history, and
  uncertainty.
- Choose the smallest complete change, not merely the smallest diff.

### While changing

- Keep the codebase capable of teaching its next reader where behavior belongs,
  what must remain true, and how to prove a change safe.
- Prefer deletion or simplification; add only when a simpler path cannot meet
  acceptance.
- Let correctness, safety, and accessibility bound simplicity.
- Localize each changeable decision and failure mode behind a clear owner and
  stable contract. A change or failure must not silently affect unrelated
  behavior; when coupling is necessary, expose it and test it.
- Match agents and tools to the task; coordination and capability must earn
  their cost.
- Act only within authority; scale evidence, oversight, and reversibility with
  risk.

### Before finishing

- A check proves only what it tests; expose assumptions, uncertainty, actions,
  and evidence.
- Stop when the full acceptance boundary is proven.
- After it works, remove every artifact not required to keep it working.

## Software design heuristics

Adapted from John Ousterhout's *A Philosophy of Software Design*.

These are defaults, not commandments. Abstraction, generality, and additional
design work must earn their cost by making likely future changes clearer or
cheaper.

- Complexity accumulates incrementally: sweat the small stuff.
- Working code is not enough; make continual, scoped investments that keep it
  understandable and changeable.
- Prefer deep modules: substantial capability behind a small interface.
- Design interfaces around common usage; simplicity at the boundary matters
  more than simplicity in the implementation.
- Generalize only when demonstrated needs make a module deeper, and keep
  general-purpose mechanisms separate from special-purpose policy.
- Give different layers different abstractions.
- Pull complexity into the module that can own it; do not merely hide it.
- Where practical, define errors and special cases out of existence.
- Design it twice when a choice is consequential or costly to reverse.
- Use comments for rationale, constraints, and other information not obvious
  from the code.
- Design software for ease of reading, not ease of writing.
- Deliver features through coherent abstractions; do not create abstractions
  detached from valuable change.
