# rug

A small web browser: GTK4 + WebKitGTK, written in Rust with relm4.

> **Heads up:** this is AI slop. Almost all of the code was written by an
> LLM (Claude) with a human steering. It works for me; read it before you
> trust it with anything.

## Build

Debian/Ubuntu dependencies:

```
sudo apt install pkg-config libgtk-4-dev libwebkitgtk-6.0-dev
```

Then:

```
cargo run                     # dev build, keeps its data in target/tmp
cargo install --path .        # release build into ~/.cargo/bin
```

Optional: `gpg` (for the password manager) and `git` (for sync).

## Using it

- **Home** (`rug://home`) has your bookmarks, frequent sites and recent
  history. Hover an item for its × to forget it.
- **URL bar** takes a URL or a search; the dropdown completes from history.
- **Tabs**: right-click a tab to open it in a new window, bookmark, mute, or
  close others. F5 reloads.
- **Bookmarks**: `rug://bookmarks` manages them. Use folders, and drag to
  reorder.
- **Private browsing**: the link on the home page, or `rug --private`. It
  saves nothing.
- **Settings** (`rug://settings`) has search engine, theme, cache, clearing
  data, the password key and the sync remote.

### Passwords

1. Pick a GPG secret key in Settings.
2. Log in somewhere; the 🔑 button by the URL bar offers to save it.
3. On a site with saved logins, click 🔑 and pick one. gpg-agent asks for
   your passphrase, then rug fills the form.

Manage or add logins at `rug://passwords`. The store uses the same layout as
[`pass`](https://www.passwordstore.org/), so `pass` can read it too.

### Sync

Settings, history, bookmarks and passwords live in one git repo:

- release: `~/.local/share/rug/profile`
- dev: `target/tmp/profile`

Set a git remote in Settings (an empty repo is fine). rug syncs on startup,
hourly, on exit, and a few seconds after a login or bookmark changes. Sync
Now forces one. Merges are done entry by entry, so there are no conflicts to
resolve.

Every machine needs the GPG secret key to read the passwords. For SSH
remotes, have your key loaded in an agent; rug won't stop to prompt.

Cookies, cache and favicons stay local to each machine.

## Hacking

See [AGENTS.md](AGENTS.md) for a map of the code and
[docs/design](docs/design) for why things are the way they are.
