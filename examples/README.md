# Examples

This directory contains small `.fx` programs used as phase artifacts for the
implementation plan in `notes/2026-05-23-implementation-phases.md`.

Examples may be written before the parser, type checker, or language server
supports every feature they use. In that state, they are goal targets and
acceptance tests for the current phase. As implementation support lands, promote
them into expected-passing parser, checker, diagnostics, hover, and
go-to-definition fixtures.

Keep examples compact and focused on the phase they belong to. Avoid reaching
for later-phase features unless the example is intentionally documenting a
boundary that should still fail.
