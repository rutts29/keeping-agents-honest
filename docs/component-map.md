# Component maps and cached review evidence

A component map is a declaration (version 1) of components. Each component has
a name, `path_globs`, requirement ids, named test commands and named end-to-end
commands. The commands are proposals; the map collector never executes them. The
existing context collector accepts a map file, and restricted collections mount
that file read-only into the existing gVisor executor.

Light commit reports can supply a component map that kah validates against the
request and its requirement IDs. The producer accepts an optional declaration in
the light report, validates its structure and exact requirement ids, and
retains the consumed request, worker and SHA provenance. It publishes the native
input in a separate read-only map directory, which keeps the observer's writable
outbox. Reports without a map remain readable; heavy gates reject the
light-only declaration. The source tree includes the component-map producer and
the verdict-cache planning code.

## Capture

Git's `:(top,glob)` pathspecs resolve paths at the exact captured HEAD. The diff
from the captured baseline includes deletions. The current committed blob IDs
and the component declaration form the map's fingerprint, independently of the
baseline. Unmapped changed paths remain explicit in the collected evidence. Names, unsafe
patterns and requirement links have typed validation errors, and no contents
are cut. Native co-change history is retained between the exact endpoints.

## Digest binding

The public map file grants no trust. The private pointer verifies its immutable
source, and capture compares that digest with the declaration returned by the
native collector. kah checks the private map record's digest this way.

## Cache validation

Cache reuse requires validated matching evidence and does not follow from a map
declaration alone. Unmapped changes remain explicit, while unrelated cached
requirements can be reused only after their own evidence checks pass. Cache
planning checks recorded component, source, authority and test-receipt
dependencies. Runtime file-read validation is not captured: runtime test reads
and failures in untouched components are marked `not yet captured`.

## Versioned Git references

These are references for Git 2.53:
[pathspec glob](https://github.com/git/git/blob/v2.53.0/Documentation/glossary-content.adoc),
[exact endpoint diff](https://github.com/git/git/blob/v2.53.0/Documentation/git-diff.adoc),
[co-change log](https://github.com/git/git/blob/v2.53.0/Documentation/git-log.adoc).
The collector uses Git glob pathspecs for diffs and an unfiltered `ls-tree`
inventory for blob metadata, because `ls-tree` rejects glob magic. There is no
custom glob matcher. The component-map and cache-planning tests show the
assertions exercised by the source tree.
