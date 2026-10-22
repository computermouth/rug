# WebKit settings

**Smooth scrolling is off.** With it on, scrolling on pages with a scrollbar
felt shuddery. Turning it off fixed it.

**One UserContentManager per webview,** carrying the autofill script and the
`rug` message handler. Related views (popups) get their own.

**The cache can be disabled** in Settings. It takes effect on the next launch,
since the network session is created at startup.
