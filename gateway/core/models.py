"""
Modelos base del simulador (dataclasses & helpers)
- ReplayConfig: configuración del replay
- Order / Position / Account: entidades simples del motor
- Helpers: now_ms, parse_interval_str, table_for_interval
"""
from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Dict, List, Optional
import time

# ----------------------- helpers -----------------------

def now_ms() -> int:
    """Epoch en milisegundos."""
    return int(time.time() * 1000)


def parse_interval_str(i: str) -> str:
    """Normaliza intervalos estilo Binance (1m, 3m, 5m, 15m, 30m, 1h, 4h, 1d)."""
    return (i or "1m").strip().lower()


def table_for_interval(i: str) -> str:
    """Devuelve la tabla DuckDB sugerida para el intervalo."""
    i = parse_interval_str(i)
    if i in ("1m", "1", "1min"):
        return "ohlc_1m"
    # Para 1h/4h/1d usaremos `ohlc` (agregado o cargado así)
    return "ohlc"


# ------------------- entidades core --------------------

@dataclass
class ReplayConfig:
    db_path: str
    symbol: str
    interval: str
    start_ts: int
    end_ts: int
    speed_bars_per_sec: float = 10.0  # velas por segundo
    maker_bps: float = 2.0
    taker_bps: float = 4.0
    slippage_bps: float = 0.0
    starting_balance: float = 100_000.0


@dataclass
class Order:
    order_id: int
    client_order_id: str
    symbol: str
    side: str            # "BUY" | "SELL"
    type: str            # "MARKET" | "LIMIT"
    qty: float
    price: Optional[float] = None
    tif: str = "GTC"
    ts: int = field(default_factory=now_ms)
    status: str = "NEW"   # NEW | FILLED | PARTIALLY_FILLED | CANCELED
    executed_qty: float = 0.0
    is_maker: Optional[bool] = None
    fills: List[Dict[str, Any]] = field(default_factory=list)  # lista de dicts estilo Binance


@dataclass
class Position:
    qty: float = 0.0
    avg_price: float = 0.0


@dataclass
class Account:
    cash: float
    position: Position = field(default_factory=Position)

    def mark_to_market(self, px: float) -> float:
        return float(self.cash) + float(self.position.qty) * float(px)

    def unrealized_pnl(self, px: float) -> float:
        if self.position.qty == 0:
            return 0.0
        return (float(px) - float(self.position.avg_price)) * float(self.position.qty)
