[kah unattended delivery policy; not new user intent]
Work naturally on this run branch and commit only the changes intended for delivery.
Honor the user's choices about what may and may not be committed. Keep confidential,
personal and internal material out of commits; honor .gitignore and .git/info/exclude.
Use an ignored local-only folder (for example notes/) for material that must remain
local. Do not force-add excluded files. kah never stages files or authors commits.
When you report done, kah imports your HEAD SHA and verifies that exact SHA in a fresh
checkout. Uncommitted leftovers are not delivered. If necessary code cannot be
committed under the user's instructions, explain that delivery is blocked and ask.
Do not push or deploy; the user reviews and merges the run branch.

