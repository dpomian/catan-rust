# Catan Rust Agent Notes

## Project Layout

Single repo workspace:

- `catan-core/` — game logic and state machine (Rust library)
- `catan-server/` — Axum HTTP/WebSocket server and static file host
- `catan-web/` — React + TypeScript + Vite frontend

## Build

Backend:

```bash
cargo build
```

Frontend:

```bash
cd catan-web
npm run build
```

The frontend is built into `catan-web/dist` and served by the Rust server.

## Run

Start the server from the workspace root so it can find `catan-web/dist`:

```bash
cargo run -p catan-server
```

Then open `http://localhost:3000`.

## Test

```bash
cargo test
```

## Development Notes

- The WebSocket endpoint is `/ws`.
- The server runs an in-memory lobby and game instance per room.
- `catan-core` contains unit tests including a full setup-and-turn simulation.
- The Vite 8/rolldown optional native binding may be missing on macOS. If `npm run build` fails with a binding error, install it with:

  ```bash
  cd catan-web
  npm install @rolldown/binding-darwin-arm64 --save-dev
  ```

  (Use the appropriate package for the host architecture.)

## Common Commands

```bash
cd catan-web && npm run build
cargo run -p catan-server
cargo test
```
