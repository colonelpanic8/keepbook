# keepbook Dioxus client

This is the first Rust UI experiment for keepbook. It is intentionally a thin
Dioxus client over the local `keepbook-server` HTTP API:

- storage, git, sync, pricing, portfolio, and formatting behavior stay in the
  existing Rust library and `keepbook::app` layer
- the browser-facing crate only owns UI state, HTTP calls, and rendering
- the API boundary is JSON so desktop/web/mobile clients can share the same
  server process later

Run the API server first:

```sh
direnv exec . cargo run -p keepbook-server -- --config ./keepbook.toml --addr 127.0.0.1:8799
```

Then serve the Dioxus web client:

```sh
direnv exec . dx serve --web --package keepbook-dioxus --addr 127.0.0.1 --port 8800 --open false
```
