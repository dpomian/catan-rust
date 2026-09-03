# Catan in Rust

A browser-based Settlers of Catan implementation with a Rust backend and a React/TypeScript frontend.

## Features

- Up to 4 human players in a WebSocket lobby.
- Randomized board with the classic 19-tile Catan island.
- Full game state machine: setup, dice rolling, resource production, robber, building, development cards, trading, and win detection.
- Player-specific game views (hidden resources and dev cards).

## Quick Start

Build the frontend:

```bash
cd catan-web
npm install
npm run build
```

Run the server:

```bash
cargo run -p catan-server
```

Open `http://localhost:3000` in your browser.

## Architecture

- `catan-core` — game rules and state machine.
- `catan-server` — Axum HTTP + WebSocket server; hosts the built web app.
- `catan-web` — React UI with an SVG board renderer.

## Tests

```bash
cargo test
```

## Notes

- The server is stateful in memory; restarting it clears active games.
- For UI development, the server serves the production build in `catan-web/dist`.
