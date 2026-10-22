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

- `rug [URL or search]` opens it in the first tab.
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

## Benchmarks

`bench/bench.py` runs rug, Firefox and Chromium through page loads of saved
real pages, Speedometer, JetStream and MotionMark, and records memory along
the way. Run it once with `--fetch`, then with no arguments. It takes over
the screen for about an hour.

One run, October 2026, on a Ryzen AI 7 PRO 360 laptop. The versions were rug
on WebKitGTK 2.52.6, Firefox ESR 140.14 and Chromium 153. Bold is best.

| | rug | Firefox | Chromium |
|---|---|---|---|
| Startup | 1.15 s | 2.35 s | **0.85 s** |
| Load Wikipedia | 271 ms | 224 ms | **146 ms** |
| Load Hacker News | **28 ms** | 37 ms | 36 ms |
| Load MDN | 127 ms | 135 ms | **121 ms** |
| Load GitHub | 348 ms | 407 ms | **336 ms** |
| Load BBC News | 278 ms | 229 ms | **182 ms** |
| Speedometer 3.1 (higher is better) | 10.07 | 9.69 | **12.74** |
| JetStream 2.2 (higher is better) | 166.4 | 115.2 | **192.7** |
| MotionMark 1.3.2 (higher is better) | 593* | 749 | **1861** |
| Memory, idle | **273 MB** | 619 MB | 408 MB |
| Memory, 5 pages open | **605 MB** | 1065 MB | 676 MB |
| Memory, peak | 2262 MB | 2114 MB | **1816 MB** |

Page loads are medians of 5, using saved copies served from localhost.
Memory is the PSS of each browser's whole process tree.

\* rug only managed about 30 fps, so its MotionMark score is very noisy.

## Hacking

See [AGENTS.md](AGENTS.md) for a map of the code and
[docs/design](docs/design) for why things are the way they are.
