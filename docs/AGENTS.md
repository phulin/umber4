# Documentation Guidance

Read the root `AGENTS.md` first.

- `architecture.md` is the high-level design proposal. It records goals,
  recommended boundaries, tradeoffs, and unresolved design questions.
- `expansion.md` develops the input and expansion loops, token-source cursors,
  argument capture and lifetime, and their paragraph-checkpoint behavior.
- Distinguish requirements, proposed choices, measured results, and open questions.
  Do not describe planned facilities as implemented.
- Keep detailed subsystem contracts in focused documents when those designs are
  developed; link them from the architecture and update this map.
- Cite primary sources for compatibility and platform facts. Record oracle
  versions and corpus identities when reporting implementation measurements.
- Use GitHub Issues for execution tracking once the repository has a remote;
  design documents are not task queues or session memory.
