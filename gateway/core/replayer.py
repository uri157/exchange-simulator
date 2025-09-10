"""
MarketReplayer
- Lee barras OHLC desde DuckDB (tabla ohlc_1m u ohlc según intervalo)
- Expone un stream asíncrono que emite (ts, o, h, l, c, v, close_ts)
- Controla la cadencia con `bars_per_sec`
- Permite reconfigurar rango/símbolo/intervalo y recargar
"""
from __future__ import annotations

import asyncio
from typing import Iterable, List, Tuple, Optional

import duckdb

from .models import table_for_interval, parse_interval_str

BarTuple = Tuple[int, float, float, float, float, float, int]


class MarketReplayer:
    def __init__(
        self,
        *,
        conn: duckdb.DuckDBPyConnection,
        symbol: str,
        interval: str,
        start_ts: int,
        end_ts: int,
        bars_per_sec: float = 10.0,
    ) -> None:
        self.conn = conn
        self.symbol = symbol.upper()
        self.interval = parse_interval_str(interval)
        self.start_ts = int(start_ts)
        self.end_ts = int(end_ts)
        self.bars_per_sec = float(bars_per_sec) if bars_per_sec else 10.0

        self._bars: List[BarTuple] = []
        self._running = False
        self._loaded = False

    # ---------------- carga de datos ----------------
    def _load_bars(self) -> None:
        tbl = table_for_interval(self.interval)
        rows = self.conn.sql(
            f"""
            SELECT ts, open, high, low, close, volume, close_ts
            FROM {tbl}
            WHERE symbol = ? AND ts >= ? AND ts <= ?
            ORDER BY ts ASC
            """,
            params=[self.symbol, self.start_ts, self.end_ts],
        ).fetchall()
        self._bars = [
            (int(a), float(b), float(c), float(d), float(e), float(f), int(g))
            for a, b, c, d, e, f, g in rows
        ]
        self._loaded = True

    def set_params(
        self,
        *,
        symbol: Optional[str] = None,
        interval: Optional[str] = None,
        start_ts: Optional[int] = None,
        end_ts: Optional[int] = None,
        bars_per_sec: Optional[float] = None,
    ) -> None:
        if symbol is not None:
            self.symbol = symbol.upper()
        if interval is not None:
            self.interval = parse_interval_str(interval)
        if start_ts is not None:
            self.start_ts = int(start_ts)
        if end_ts is not None:
            self.end_ts = int(end_ts)
        if bars_per_sec is not None and bars_per_sec > 0:
            self.bars_per_sec = float(bars_per_sec)
        self._loaded = False

    @property
    def bars_count(self) -> int:
        return len(self._bars)

    # ---------------- bucle de replay ----------------
    async def stream(self) -> Iterable[BarTuple]:
        """Async generator de barras a `bars_per_sec`.
        Si no hay barras, retorna inmediatamente.
        """
        if not self._loaded:
            self._load_bars()
        if not self._bars:
            return
        self._running = True
        delay = 1.0 / max(self.bars_per_sec, 0.001)
        for bar in self._bars:
            if not self._running:
                break
            yield bar
            await asyncio.sleep(delay)
        self._running = False

    def stop(self) -> None:
        self._running = False
