# Game3 browser deployment evidence

Verified 2026-09-08. Implementation: `3c452b6`, `f99fc15`, `6b6395c`.

- Play: https://hafley.codes/game3/
- Scripted proof: https://hafley.codes/game3/?demo=1
- Repeat deployment from this directory: `just web-deploy`.
- Setup dependencies: `just web-setup`; toolchain prerequisites: `94_web/README.md`.

## Executed checks

Native regression: 26 Rust tests and 360 golden worlds passed. Native Godot typed
payload and keyboard checks passed. Local and production Chromium both executed
300 ticks with exact native presentation-row equality, SQLite readback and mesh
receipts, one hit for 18 damage, and 120 exact restored/replayed simulation states.
Keyboard movement, jump, fair and synthetic on-screen touch input passed. Browser
receipt reports no runtime errors. Production hit screenshot was visually inspected.

Production receipt and H.264 MP4:

```text
/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/falcon-browser-eA9ZLG/receipt.json
/var/folders/z2/cwfm40fn65n176q8m227wl0r0000gn/T/falcon-browser-eA9ZLG/proof.mp4
```

MP4: 12.76 seconds, 557167 bytes. Temporary artifacts can be reclaimed by the OS.
Publication log: `/private/tmp/falcon-web-publish.log`.
Ignored deployment receipt: `94_web/build/published.json` records artifact hashes.
Retained server backup: `/var/www/smash-godot-game3-backup-20260908204010205`.
Every protected `/var/www/smash-godot/` regular-file hash matched before and after.
No nginx configuration changes or reload were performed.

## Failures exercised

The initial system rsync rejected `--delete-delay`; use supported `--delete-after`
with delayed updates. A subsequent browser check observed simulation completion
before its fixed recording wait expired; acceptance now waits for the actual
`CONTROL_CAPTURE_OK` console event. Both failed attempts restored the previous
Game3 directory and verified protected hashes. The final retry passed.

## Scope remaining

Physical phones, Safari, online play and cross-browser determinism remain
unverified. Emscripten emits a main-thread blocking warning. This proof establishes
neither zero-copy nor zero-allocation execution. Source ingest runs offline and
baked assets decode once at startup. Existing lab regression equality does not
establish Melee/PM behavior equivalence. Behavior importer coverage and photo tools
remain the next roadmap items.
