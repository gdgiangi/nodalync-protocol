# Nodalync Studio

A native Tauri 2 application for local notes, content imports, and a knowledge
graph. The Rust backend uses Nodalync's protocol crates directly. Browsing the
Vite URL by itself does not provide the native backend.

## Run locally

Install Node.js, Rust, and your platform's Tauri build prerequisites. On macOS,
Xcode Command Line Tools are sufficient for the desktop target. Run these
commands from `apps/desktop`:

```sh
npm ci
RUSTUP_TOOLCHAIN=1.98.0 npm run native:dev
```

`native:dev` starts Vite on port 1420 and opens the native application with hot
reload. The active Rust manifest and `tauri.conf.json` are both in this directory;
the historical `src-tauri` scaffold is not the application manifest.

To open the native app with compiled frontend assets and no Vite server:

```sh
RUSTUP_TOOLCHAIN=1.98.0 npm run native
```

The app creates or unlocks a local encrypted identity before content operations.
Creating a note or importing text stores private content on this device. Starting
a network connection is a separate action.

## Isolated local profile

Use an absolute directory for a separate development profile, without changing
your normal Nodalync Studio identity or content:

```sh
NODALYNC_DATA_DIR=/tmp/nodalync-studio-dev-profile \
RUSTUP_TOOLCHAIN=1.98.0 npm run native:dev
```

By default, the graph database is `studio/knowledge.db` within that profile. Set
`NODALYNC_GRAPH_DB` to an explicit database path to inspect a chosen graph.
The app does not automatically open the repository's tracked sample databases.
Without overrides, node data uses the operating system's application-data
directory for `com.nodalync.studio`.

## Validation

```sh
npm run build
npm test
cargo +1.98.0 check --locked --manifest-path Cargo.toml
cargo +1.98.0 test --locked --manifest-path Cargo.toml
```

Native dialog permissions are scoped to the main window. The bundled document
picker imports local text/Markdown files; local note reads do not contact peers
or initiate payments.

## Native UI test fixture and local app bundle

A development-only example creates three synthetic private notes and a small
linked graph through the real protocol and graph APIs. It refuses to replace
an existing identity or populate a graph that already contains entities:

```sh
cargo +1.98.0 run --locked --example seed_dev_profile -- /tmp/nodalync-studio-dev-profile
```

The example prints its synthetic test-only unlock password. It does not start
networking or settle payments.

For native app discovery by macOS tools, build and wrap the development binary:

```sh
cargo +1.98.0 build --locked --no-default-features
python3 scripts/bundle-dev-app.py /tmp/nodalync-studio-dev-profile
```

This creates and registers `target/debug/bundle/macos/Nodalync Studio.app` without
launching it. Its launcher uses that isolated profile and the existing Vite
server on port 1420. Close the app before rebuilding the bundle. The bundle stays
under the ignored Cargo target directory.
