"""
Estado compartido del simulador (SimState)
- Mantiene conexión DuckDB, cuenta, executor, run store
- Gestiona clientes WS y broadcast
- Orquesta el replayer (barras → fills + eventos WS)
"""
from __future__ import annotations

import asyncio
import contextlib
import json
from typing import Any, Dict, List, Set

import duckdb
from fastapi import WebSocket

from .models import ReplayConfig, Account, Order, parse_interval_str, now_ms
from .store import RunStore
from .executor import Executor
from .replayer import MarketReplayer


class SimState:
    def __init__(self, cfg: ReplayConfig):
        self.cfg = cfg
        self.con: duckdb.DuckDBPyConnection = duckdb.connect(cfg.db_path, read_only=False)
        self.store = RunStore(self.con)
        self.account = Account(cash=cfg.starting_balance)
        self.exec = Executor(
            account=self.account,
            store=self.store,
            maker_bps=cfg.maker_bps,
            taker_bps=cfg.taker_bps,
            slippage_bps=cfg.slippage_bps,
        )

        self.symbol = cfg.symbol.upper()
        self.interval = parse_interval_str(cfg.interval)

        self.ws_clients: Set[WebSocket] = set()
        self.running = False
        self.replay_task: asyncio.Task | None = None
        self.cur_price: float = 0.0

        # meta de cuenta (para endpoints tipo leverage/margin/dual)
        self.leverage: int = 1
        self.margin_type: str = "cross"
        self.dual_side: bool = True

        # Replayer (carga de barras + loop de emisión)
        self.replayer = MarketReplayer(
            conn=self.con,
            symbol=self.symbol,
            interval=self.interval,
            start_ts=self.cfg.start_ts,
            end_ts=self.cfg.end_ts,
            bars_per_sec=self.cfg.speed_bars_per_sec,
        )

        # Crear run al iniciar
        params = {
            "symbol": self.symbol,
            "interval": self.interval,
            "start_ts": self.cfg.start_ts,
            "end_ts": self.cfg.end_ts,
            "maker_bps": self.cfg.maker_bps,
            "taker_bps": self.cfg.taker_bps,
            "slippage_bps": self.cfg.slippage_bps,
            "starting_balance": self.cfg.starting_balance,
            "speed_bars_per_sec": self.cfg.speed_bars_per_sec,
        }
        self.run_id = self.store.new_run(strategy="gateway/binance-sim", params=params)

    # ---------------- control del ciclo ----------------

    async def start(self) -> None:
        if self.running:
            return
        self.running = True
        async def _loop():
            async for bar in self.replayer.stream():
                ts, o, h, l, c, v, cts = bar
                await self._on_bar(ts, o, h, l, c, v, cts)
                if not self.running:
                    break
        self.replay_task = asyncio.create_task(_loop())

    async def stop(self) -> None:
        self.running = False
        if self.replay_task:
            self.replay_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self.replay_task
            self.replay_task = None

    # ---------------- websockets -----------------------

    async def register_ws(self, ws: WebSocket) -> None:
        self.ws_clients.add(ws)

    def unregister_ws_sync(self, ws: WebSocket) -> None:
        self.ws_clients.discard(ws)

    async def broadcast(self, msg: Dict[str, Any]) -> None:
        data = json.dumps(msg)
        dead: List[WebSocket] = []
        for ws in list(self.ws_clients):
            try:
                await ws.send_text(data)
            except Exception:
                dead.append(ws)
        for ws in dead:
            self.unregister_ws_sync(ws)

    # ---------------- manejo de barra ------------------

    async def _on_bar(self, ts: int, o: float, h: float, l: float, c: float, v: float, cts: int) -> None:
        # precio corriente (abre de la barra)
        self.cur_price = float(o)

        # cruzar LIMITs con OHLC de la barra
        self.exec.on_bar(bar_open=o, bar_high=h, bar_low=l, ts_close=cts, symbol=self.symbol)

        # equity al close
        equity = self.account.mark_to_market(c)
        self.store.log_equity(cts, equity)

        # evento kline cerrado
        k = {
            "e": "kline", "E": now_ms(), "s": self.symbol,
            "k": {
                "t": ts, "T": cts, "s": self.symbol, "i": self.interval,
                "o": f"{o:.8f}", "c": f"{c:.8f}", "h": f"{h:.8f}", "l": f"{l:.8f}",
                "v": f"{v:.8f}", "n": 0, "x": True, "q": "0", "V": "0", "Q": "0", "B": "0"
            }
        }
        await self.broadcast({
            "stream": f"{self.symbol.lower()}@kline_{self.interval}",
            "data": k
        })

        # evento markPriceUpdate (simple: usa close)
        await self.broadcast({
            "stream": f"{self.symbol.lower()}@markPrice@1s",
            "data": {"e": "markPriceUpdate", "E": now_ms(), "s": self.symbol, "p": f"{c:.8f}"}
        })
"""
Estado compartido del simulador (SimState)
- Mantiene conexión DuckDB, cuenta, executor, run store
- Gestiona clientes WS y broadcast
- Orquesta el replayer (barras → fills + eventos WS)
"""

import asyncio
import contextlib
import json
from typing import Any, Dict, List, Set

import duckdb
from fastapi import WebSocket

from .models import ReplayConfig, Account, Order, parse_interval_str, now_ms
from .store import RunStore
from .executor import Executor
from .replayer import MarketReplayer


class SimState:
    def __init__(self, cfg: ReplayConfig):
        self.cfg = cfg
        self.con: duckdb.DuckDBPyConnection = duckdb.connect(cfg.db_path, read_only=False)
        self.store = RunStore(self.con)
        self.account = Account(cash=cfg.starting_balance)
        self.exec = Executor(
            account=self.account,
            store=self.store,
            maker_bps=cfg.maker_bps,
            taker_bps=cfg.taker_bps,
            slippage_bps=cfg.slippage_bps,
        )

        self.symbol = cfg.symbol.upper()
        self.interval = parse_interval_str(cfg.interval)

        self.ws_clients: Set[WebSocket] = set()
        self.running = False
        self.replay_task: asyncio.Task | None = None
        self.cur_price: float = 0.0

        # meta de cuenta (para endpoints tipo leverage/margin/dual)
        self.leverage: int = 1
        self.margin_type: str = "cross"
        self.dual_side: bool = True

        # Replayer (carga de barras + loop de emisión)
        self.replayer = MarketReplayer(
            conn=self.con,
            symbol=self.symbol,
            interval=self.interval,
            start_ts=self.cfg.start_ts,
            end_ts=self.cfg.end_ts,
            bars_per_sec=self.cfg.speed_bars_per_sec,
        )

        # Crear run al iniciar
        params = {
            "symbol": self.symbol,
            "interval": self.interval,
            "start_ts": self.cfg.start_ts,
            "end_ts": self.cfg.end_ts,
            "maker_bps": self.cfg.maker_bps,
            "taker_bps": self.cfg.taker_bps,
            "slippage_bps": self.cfg.slippage_bps,
            "starting_balance": self.cfg.starting_balance,
            "speed_bars_per_sec": self.cfg.speed_bars_per_sec,
        }
        self.run_id = self.store.new_run(strategy="gateway/binance-sim", params=params)

    # ---------------- control del ciclo ----------------

    async def start(self) -> None:
        if self.running:
            return
        self.running = True
        async def _loop():
            async for bar in self.replayer.stream():
                ts, o, h, l, c, v, cts = bar
                await self._on_bar(ts, o, h, l, c, v, cts)
                if not self.running:
                    break
        self.replay_task = asyncio.create_task(_loop())

    async def stop(self) -> None:
        self.running = False
        if self.replay_task:
            self.replay_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self.replay_task
            self.replay_task = None

    # ---------------- websockets -----------------------

    async def register_ws(self, ws: WebSocket) -> None:
        self.ws_clients.add(ws)

    def unregister_ws_sync(self, ws: WebSocket) -> None:
        self.ws_clients.discard(ws)

    async def broadcast(self, msg: Dict[str, Any]) -> None:
        data = json.dumps(msg)
        dead: List[WebSocket] = []
        for ws in list(self.ws_clients):
            try:
                await ws.send_text(data)
            except Exception:
                dead.append(ws)
        for ws in dead:
            self.unregister_ws_sync(ws)

    # ---------------- manejo de barra ------------------

    async def _on_bar(self, ts: int, o: float, h: float, l: float, c: float, v: float, cts: int) -> None:
        # precio corriente (abre de la barra)
        self.cur_price = float(o)

        # cruzar LIMITs con OHLC de la barra
        self.exec.on_bar(bar_open=o, bar_high=h, bar_low=l, ts_close=cts, symbol=self.symbol)

        # equity al close
        equity = self.account.mark_to_market(c)
        self.store.log_equity(cts, equity)

        # evento kline cerrado
        k = {
            "e": "kline", "E": now_ms(), "s": self.symbol,
            "k": {
                "t": ts, "T": cts, "s": self.symbol, "i": self.interval,
                "o": f"{o:.8f}", "c": f"{c:.8f}", "h": f"{h:.8f}", "l": f"{l:.8f}",
                "v": f"{v:.8f}", "n": 0, "x": True, "q": "0", "V": "0", "Q": "0", "B": "0"
            }
        }
        await self.broadcast({
            "stream": f"{self.symbol.lower()}@kline_{self.interval}",
            "data": k
        })

        # evento markPriceUpdate (simple: usa close)
        await self.broadcast({
            "stream": f"{self.symbol.lower()}@markPrice@1s",
            "data": {"e": "markPriceUpdate", "E": now_ms(), "s": self.symbol, "p": f"{c:.8f}"}
        })
