# keepbook Dioxus client

This is the first Rust UI experiment for keepbook. The desktop build runs the
UI against the native keepbook API layer in-process:

- storage, git, sync, pricing, portfolio, and formatting behavior stay in the
  existing Rust library and `keepbook::app` layer
- the UI crate only owns UI state and rendering
- the web/mobile HTTP API boundary is JSON so clients can share the same server
  process later

Serve the Dioxus desktop app:

```sh
direnv exec . just dioxus-serve
```

The web client still uses the local `keepbook-server` HTTP API. Run the API
server first:

```sh
direnv exec . cargo run -p keepbook-server -- --addr 127.0.0.1:8799
```

Then serve the Dioxus web client:

```sh
direnv exec . dx serve --web --package keepbook-dioxus --addr 127.0.0.1 --port 8800 --open false
```

Build an iOS simulator app bundle:

```sh
direnv exec . just dioxus-ios-build
```

The bundle is written to:

```sh
target/dx/keepbook-dioxus/debug/ios/KeepbookDioxus.app
```
