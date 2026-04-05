# Exchange Simulator

Exchange Simulator is a deterministic, Binance-inspired trading environment for developing and validating bots against historical market data before touching real capital.

It is designed as an engineering platform, not just a chart replay tool: dataset ingestion, session orchestration, exchange-like order endpoints, bot identity, run tracking, and UI control surfaces are all part of the same system.

---

## System Purpose

Most strategy failures appear in integration boundaries, not in indicator math:

- replay timing drift
- order handling under accelerated clocks
- bot/runtime disconnects
- inconsistent analytics across runs

Exchange Simulator focuses on those boundaries with reproducible, session-scoped execution and explicit run history.

---

## Repositories

- **Engine (Rust API + replay core)**  
  https://github.com/uri157/exchange-simulator-backend
- **Control Surface (Next.js UI)**  
  https://github.com/uri157/exchange-simulator-frontend

This repository provides the high-level project overview.

---

## Conceptual Model

- **Dataset**: historical market source for a symbol (ingested as aggTrade-first data).
- **Session**: deterministic replay envelope (symbols, interval, speed, fee model, time bounds).
- **Run**: one execution lifecycle of a session (`start -> stop/completion`).
- **Bot**: identity + credentials that can bind to sessions and produce run statistics.
- **Live Runtime Telemetry**: operational health signal emitted by running bots.

The system intentionally separates:

- **Simulation Plane** (deterministic backtesting and replay)
- **Live Operations Plane** (runtime health and bot telemetry)

---

## Architecture at a Glance

1. **Data Plane**
   - Historical aggTrades are ingested into TimescaleDB/PostgreSQL.
   - Klines are derived on demand and during replay from aggTrades.
2. **Execution Plane**
   - Replay engine advances simulated time per session.
   - Session lifecycle controls (`start`, `pause`, `resume`, `stop`) drive run creation/finalization.
3. **Trading Plane**
   - Binance-like REST endpoints for account/orders/trades.
   - Deterministic fill rules with session-scoped fees.
4. **Control Plane**
   - Bot management, session orchestration, and run analytics.
5. **Telemetry Plane**
   - Runtime telemetry ingestion and fleet snapshots for live monitoring.

---

## Compatibility Strategy

The project follows a pragmatic Binance-compatibility approach:

- familiar endpoint shape for bot integration
- deterministic behavior over exchange-perfect microstructure
- explicit room for future realism upgrades (ticks, partial fills, richer matching)

Goal: reduce integration friction while keeping simulation semantics controlled and reproducible.

---

## Current Scope

Implemented and production-oriented:

- aggTrade-first storage model on TimescaleDB
- session-centric run lifecycle
- bot binding and run-linked trade history
- live telemetry ingestion and fleet snapshots
- modern UI for datasets, sessions, bots, lab analytics, and live views

Planned iterations focus on deeper exchange realism and expanded bot operations tooling.

