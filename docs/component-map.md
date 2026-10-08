# Component map — C, native inventory slice

The thinker-maintained declaration is version1 with components containing name,
path_globs, requirement ids, named test commands and named E2E commands. These
commands are proposals; the map collector never executes them. The existing
context collector accepts a map file, and restricted collections mount that
file read-only into the existing gVisor executor.

Git's native `:(top,glob)` pathspecs resolve paths at the exact captured HEAD.
The diff from the captured baseline includes deletions. Current committed blob
IDs and the component declaration form its fingerprint, independently of the
baseline. Unmapped changed paths stay explicit; they must receive a scope/check
decision before caching can consider unaffected work. Names, unsafe patterns
and requirement links have typed validation errors. No contents are cut.

Native co-change history is retained between the exact endpoints. Runtime test
reads and failures in untouched components are currently marked **not captured**.
The pushed0afe240 producer accepts an optional declaration in the existing light
report, validates its structure and exact requirement ids, and retains consumed
request/worker/SHA provenance. It publishes native input in a separate read-only
map directory, preserving the observer's writable outbox. Legacy map-free
reports remain readable; heavy gates reject the light-only declaration.
The earlier foundation does not yet include this producer. Runtime
dependency validations and cache planning/carry remain unfinished. Public/private
digest binding is implemented in the working tree and is being checked: the
private pointer verifies its immutable source, and capture compares that digest
with the native collector's returned declaration. A public-only file grants no
trust. No C/KEEP-UP pass or cache-reuse claim is made.

Official installed-version references:
[pathspec glob](https://github.com/git/git/blob/v2.53.0/Documentation/glossary-content.adoc),
[exact endpoint diff](https://github.com/git/git/blob/v2.53.0/Documentation/git-diff.adoc),
[co-change log](https://github.com/git/git/blob/v2.53.0/Documentation/git-log.adoc).
Installed Git2.53 confirms diff supports glob magic; ls-tree rejects it, so it is
used only for the unfiltered metadata inventory. No custom glob matcher or new
dependency was added. Frozen cases precede implementation in phase-5o-cache.
