# Security

Report vulnerabilities privately to the maintainers rather than in a public
issue. Include steps to reproduce and the affected version.

Measures in this crate: strict identifier parsing; a locator scheme
allow-list; size limits on all text and collections; control-character
rejection; future-timestamp rejection; typed extensions only; imports verified
by hash and replay and gated by an origin policy; viewer filtering of
restricted records, events, sources and withheld identities; redaction that
scrubs history. Authorization is the application's responsibility.
