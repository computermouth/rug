# Profile and paths

**One directory for everything worth keeping.** Settings, history, bookmarks
and passwords live in the profile directory (`~/.local/share/rug/profile`,
or `target/tmp/profile` for debug builds). That directory is a git repo, so
it syncs between machines.

**Machine-local things stay out.** Cookies, WebKit storage, the cache and
favicons live outside the profile. They're large or churn constantly, and
cookies shouldn't travel.

**Debug builds never touch real data.** `paths.rs` picks `target/tmp` when
built with debug assertions. All paths go through it.

**JSON files.** Everything is pretty-printed JSON. That keeps git diffs
readable and lets the merge driver work.

- History: `[url, title, time_ms]` entries, newest first, capped at 1000.
- Bookmarks: a flat list of items with random ids and parent ids (see
  sync.md).

**Migration.** `paths::migrate` moves files from where older versions kept
them. Formats parse old shapes (untagged serde enums) rather than versioning.
