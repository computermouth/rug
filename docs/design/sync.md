# Sync

**git, driven by the CLI.** We shell out to `git -C <profile>`; no libgit2.
Any remote git understands works, with the user's own SSH and credentials
setup.

**When.**
- Commit after 30s of quiet (history changes constantly).
- Sync on startup, hourly, on exit, and 5s after a login or bookmark change.
- Syncs are serialized; a request during a sync runs one more afterwards.

**Never stop at a conflict.** Nobody is going to resolve a merge conflict in
a browser profile. rug registers itself as a git merge driver
(`rug --merge-driver %O %A %B %P`, with `* merge=rug` in `.gitattributes`).
That also covers a manual `git pull`. Each file merges by entry:

- **settings.json**: field by field. Take whichever side changed a field; if
  both did, ours wins.
- **history.json**: keyed by URL. The newer visit wins. A deletion sticks
  unless the other side visited again after the common base.
- **bookmarks.json**: keyed by id. If both sides changed a bookmark, ours
  wins. A deletion beats an unchanged copy. Order comes from whichever side
  reordered. Orphans and parent loops go to the top level.
- **passwords** (encrypted): keep ours. Both sides saving the same login at
  once is rare; the other version is still in git history.

Whatever the driver can't settle (one side deletes a file the other edited)
is resolved by keeping whatever is in the working tree.

**Random bookmark ids** (under 2^53, so they're safe as JS numbers).
Sequential ids would collide when two machines add bookmarks offline.

**Non-interactive.** `GIT_TERMINAL_PROMPT=0` and ssh `BatchMode`, with a
connect timeout. Sync reports an error instead of hanging, which matters
most on exit.
