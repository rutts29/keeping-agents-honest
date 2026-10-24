# Component maps and cached review evidence

A component map tells the observer which parts of a project a change touches.
The native observer maintains it alongside its reviews, and kah uses it to
attribute changed paths to components and their requirements, and to plan when
an earlier review verdict can be reused. This page covers what a map contains,
how kah validates it, and what cache reuse requires.

## What a map declares

A component map (format version 1) lists components. Each component has:

- a name and `path_globs`;
- the requirement IDs it relates to;
- named test commands and named end-to-end commands.

The commands are proposals. The map collector never runs them. The context
collector accepts a map file, and restricted collections mount it read-only
into the gVisor executor.

## Where a map comes from

The observer's light commit report (its per-commit review report) can carry a
map declaration. kah validates the declaration's structure and its exact
requirement IDs against the review request, and keeps the request, worker and
SHA provenance. It publishes the map in a separate read-only directory, so the
observer's writable outbox stays separate. Reports without a map are still
read. Only light commit reports may submit a map declaration; full gate
reports must omit that field.

## Capture

- Paths resolve with Git `:(top,glob)` pathspecs at the exact captured HEAD.
- The diff from the captured baseline includes deletions.
- The map's fingerprint is the current committed blob IDs plus the component
  declaration, independent of the baseline.
- Changed paths that no component claims stay explicit in the evidence.
- Invalid names, unsafe patterns and bad requirement links are typed
  validation errors. No contents are cut.
- Co-change history between the exact endpoints is kept.

The collector uses Git glob pathspecs for diffs and an unfiltered `ls-tree`
inventory for blob metadata, because `ls-tree` rejects glob magic. There is no
custom glob matcher.

## Digest binding

The map file published in the observer workspace grants no trust by itself.
A private pointer records the map's immutable source, and capture compares
that digest with the declaration the native collector returns.

## Cache reuse

A map alone never allows a cached verdict to be reused. Reuse requires
validated, matching evidence:

- Unmapped changes stay explicit.
- A cached verdict for an unrelated requirement is reused only after its own
  evidence checks pass.
- Cache planning checks the recorded component, source, authority and
  test-receipt dependencies.
- Files read by tests at runtime are not captured. Runtime test reads, and
  failures in untouched components, are marked `not yet captured`.

## References

Git 2.53 documentation:
[pathspec glob](https://github.com/git/git/blob/v2.53.0/Documentation/glossary-content.adoc),
[exact endpoint diff](https://github.com/git/git/blob/v2.53.0/Documentation/git-diff.adoc),
[co-change log](https://github.com/git/git/blob/v2.53.0/Documentation/git-log.adoc).
