# Godot web driver

`createWebApp({ root, describe, head, profile, packageName, requiredSprites, webAssets,
reloadCommand })` returns `prepare`, `publishCommands`, `serve`, and async
`main(profileName, action)`.

One instance owns one application's absolute root. Profile parsing/target validation,
HTML additions and optional web assets belong to the consumer. The driver owns
native + Emscripten builds, Godot export, compression, profile receipts, loopback
serving/probes and rsync/SSH publishing. Only `publish` changes the remote deployment.

Expected application layout: `Cargo.toml`, `rust-toolchain.toml`, `.cargo/`,
`godot/`, `deploy/profiles/`, `deploy/web/`, `build/`, and local `target/`.
Both builds explicitly use the application's target directory. Tool-path overrides
are `GODOT45`, `EMSDK_ENV`, or ignored per-profile local env files.

Consumer: games/kneeman/app/deploy/scripts/1_web.mjs.
Existing integration tests: run `just web-check` from games/kneeman.
Source: kneeman recovery 8950de7e, deploy/scripts/1_web.mjs.
