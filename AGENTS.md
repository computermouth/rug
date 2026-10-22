# AGENTS.md

Notes for anyone (human or LLM) changing rug.

## Layout

| Path | What |
|---|---|
| `src/main.rs` | Startup and shutdown, and the `--merge-driver` entry point |
| `src/window.rs` | Browser window: toolbar, tabs, 🔑 password popover |
| `src/tab.rs` | One tab: its webview, tab label, context menus, login capture |
| `src/web.rs` | WebKit setup: network session, per-view settings, injected scripts |
| `src/pages.rs` | The `rug://` pages (home, settings, bookmarks, passwords) |
| `src/completion.rs` | The URL bar's history dropdown |
| `src/bookmarks.rs`, `src/bookmark_dialog.rs` | Bookmark tree, its merge, and the add dialog |
| `src/store.rs` | Settings, history and favicons |
| `src/passwords.rs` | The gpg / `pass`-compatible password store |
| `src/sync.rs` | The profile git repo: auto-commit, sync, and the merge driver |
| `src/paths.rs` | Every on-disk location |
| `src/urls.rs` | URL and query-string helpers |
| `assets/pages/` | HTML, CSS and JS for the `rug://` pages, filled from `{{name}}` placeholders |
| `assets/autofill.js` | Login capture and filling, injected into pages |

## Rules of thumb

- Read [docs/design](docs/design) before changing how data is stored,
  synced or merged.
- Debug builds must never touch the real profile. Every path goes through
  `paths.rs`.
- Anything that runs `gpg` or `git` blocks. Run it with
  `gio::spawn_blocking`, never on the GTK thread.
- Files in the profile need a merge in `sync::merge_driver` (or a reason to
  keep our side), because sync must never stop at a conflict.
- Match the existing style: short doc comments that say why, no comment
  noise, `cargo clippy` clean.

## Testing

There are no automated tests. Run `cargo run` and click around. The dev
profile lives in `target/tmp/profile`; delete it to start fresh. For
passwords, use a throwaway key with `GNUPGHOME=/some/tmp/dir`.
