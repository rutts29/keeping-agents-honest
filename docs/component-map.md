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
The observer map producer, those validations and cache planning/carry are still
unfinished. No C/KEEP-UP pass or cache-reuse claim is made by this slice.

Official installed-version references:
[pathspec glob](https://github.com/git/git/blob/v2.53.0/Documentation/glossary-content.adoc),
[exact endpoint diff](https://github.com/git/git/blob/v2.53.0/Documentation/git-diff.adoc),
[co-change log](https://github.com/git/git/blob/v2.53.0/Documentation/git-log.adoc).
Installed Git2.53 confirms diff supports glob magic; ls-tree rejects it, so it is
used only for the unfiltered metadata inventory. No custom glob matcher or new
dependency was added. Frozen cases precede implementation in phase-5o-cache.
