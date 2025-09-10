"""
Utils de tiempo para el simulador / gateway
- now_ms(): epoch ms
- to_ms(s): parsea epoch(s|ms), ISO 8601 (con/sin Z) o YYYY-MM-DD
"""
from __future__ import annotations

from datetime import datetime, timezone


def now_ms() -> int:
    """Epoch en milisegundos."""
    import time
    return int(time.time() * 1000)


def to_ms(s: str) -> int:
    """Convierte entero/ISO/fecha a epoch ms.
    - Si es dígito: asume ms o s (si < 1e10, multiplica por 1_000)
    - ISO 8601 con/ sin zona: "2024-07-01T00:00:00Z" | "2024-07-01 12:34:56"
    - Fecha YYYY-MM-DD (00:00:00Z)
    """
    s = s.strip()
    if s.isdigit():
        v = int(s)
        return v if v > 10_000_000_000 else v * 1000
    # ISO (admite Z)
    s_iso = s.replace("Z", "+00:00").replace(" ", "T")
    try:
        dt = datetime.fromisoformat(s_iso)
    except Exception:
        dt = datetime.strptime(s, "%Y-%m-%d")
        dt = dt.replace(tzinfo=timezone.utc)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return int(dt.timestamp() * 1000)
