# Exchange Simulator

A deterministic **Binance-like exchange simulator** designed for strategy backtesting using historical market data.

The project allows trading bots to interact with a local simulated exchange that exposes a **Binance-inspired REST API and WebSocket streams**, enabling reproducible testing without connecting to real exchanges.

---

## Architecture

The system is composed of two main components:

### Backend
Rust service that simulates an exchange environment.

Features:
- REST API compatible with common Binance endpoints
- WebSocket kline streaming
- Historical market replay engine
- Session-based simulation
- Deterministic order execution
- DuckDB storage for datasets

Repository:
https://github.com/tuusuario/exchange-simulator-backend

---

### Frontend
Next.js application used to visualize sessions and interact with the simulator.

Features:
- Session management UI
- WebSocket live candle streaming
- Replay controls (start, pause, resume, seek)
- Debug information for WebSocket connections

Repository:
https://github.com/tuusuario/exchange-simulator-frontend

---

## Typical Use Case

1. Load historical market data into DuckDB
2. Start the exchange simulator backend
3. Create a replay session
4. Connect a trading bot or the frontend UI
5. Replay historical markets deterministically

This enables testing strategies over years of data without interacting with real exchanges.

---

## Project Goals

- Deterministic trading simulation
- Fast historical data replay
- Exchange-like environment for bot testing
- Simple architecture that can be extended with indicators, metrics and persistence

---

## Roadmap

- Persistent accounts and orders
- Metrics (Prometheus / OpenTelemetry)
- Strategy benchmarking tools
- More realistic exchange mechanics


