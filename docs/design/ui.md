# UI

**relm4 for the GTK side.** Each piece (window, tab, completion dropdown,
bookmark dialog) is a component with a message enum. State changes go through
`update`, not callbacks reaching into widgets.

**Internal pages are HTML served from `rug://`.** Home, settings, bookmarks
and passwords are templates in `assets/pages/` with `{{name}}` placeholders,
filled in by `pages.rs`. Building them in HTML is much faster than in GTK,
and they look like the web pages next to them.

- Actions are plain links or form submits back to `rug://...` with a query
  string. The handler acts, then redirects, so reloading doesn't repeat the
  action.
- Forms carrying a password use POST, so the password never sits in a URL
  or in history.
- The handler is async because some actions take a while (sync,
  re-encrypting passwords).
- The pages have no access to anything special. Everything they can do is a
  URL that rug handles.
