# Passwords

**`pass`-compatible storage.** `passwords/<host>/<username>.gpg` holds the
password on its first line, then `login:` and `url:` lines. The key id is in
`passwords/.gpg-id`. Existing tools (`pass`, its phone apps) can read the
store, and listing logins needs no decryption.

**gpg via the CLI, passphrase via gpg-agent.** rug never sees the
passphrase. Pinentry asks, and the agent's cache decides how long it's
remembered. Encrypting needs no passphrase, so saving never prompts.

**Changing keys re-encrypts everything.** It decrypts everything first, so a
wrong passphrase changes nothing.

**Autofill runs in an isolated script world.** `assets/autofill.js` is
injected into the `rug` world, so the page can't see it, call `rugFill`, or
post fake logins to rug's message handler.

- It notices logins on submit, on a click on a submit-like button, or on
  Enter.
- Two-step logins: the username is remembered in sessionStorage for the
  password page.
- Verification-code fields are ignored. That covers ones that look like
  codes, and a password-only page right after a full login.
- Fills set values through the native setter and fire input/change events,
  so React and similar frameworks notice.

**Offers are conservative.** No offer in private mode, on non-http pages, on
"never" sites, or for a login rug just filled. Logins match the saved host
and its subdomains; `www.` is stripped.
