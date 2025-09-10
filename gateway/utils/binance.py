"""
Helpers de formato/eventos estilo Binance
- kline_close_event(...): payload de kline cerrado (x=true)
- mark_price_update(...): payload de markPriceUpdate
- stream_envelope(stream, data): wrapper {"stream":..., "data":...}
- build_stream_url(base, symbols, intervals, include_mark_price=True)
"""
from __future__ import annotations

from typing import Dict, List, Optional

from .time import now_ms


def kline_close_event(
    *,
    symbol: str,
    interval: str,
    ts_open: int,
    ts_close: int,
    o: float,
    h: float,
    l: float,
    c: float,
    v: float,
    event_time_ms: Optional[int] = None,
) -> Dict:
    return {
        "e": "kline",
        "E": int(event_time_ms or now_ms()),
        "s": symbol.upper(),
        "k": {
            "t": int(ts_open),
            "T": int(ts_close),
            "s": symbol.upper(),
            "i": str(interval).lower(),
            "o": f"{float(o):.8f}",
            "c": f"{float(c):.8f}",
            "h": f"{float(h):.8f}",
            "l": f"{float(l):.8f}",
            "v": f"{float(v):.8f}",
            "n": 0,
            "x": True,
            "q": "0",
            "V": "0",
            "Q": "0",
            "B": "0",
        },
    }


def mark_price_update(symbol: str, price: float, event_time_ms: Optional[int] = None) -> Dict:
    return {
        "e": "markPriceUpdate",
        "E": int(event_time_ms or now_ms()),
        "s": symbol.upper(),
        "p": f"{float(price):.8f}",
    }


def stream_envelope(stream: str, data: Dict) -> Dict:
    return {"stream": stream, "data": data}


def build_stream_url(base: str, symbols: List[str], intervals: List[str], include_mark_price: bool = True) -> str:
    parts: List[str] = []
    for s in symbols:
        for iv in intervals:
            parts.append(f"{s.lower()}@kline_{iv}")
        if include_mark_price:
            parts.append(f"{s.lower()}@markPrice@1s")
    streams = "/".join(parts)
    base = base.rstrip("/")
    return f"{base}/stream?streams={streams}"
