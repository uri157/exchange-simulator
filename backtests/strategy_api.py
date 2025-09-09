from __future__ import annotations
from typing import Protocol, Any, Optional
from sim.models import Bar


class Strategy(Protocol):
    """
    Contrato mínimo: el runner llamará on_start(), on_bar(bar), on_finish().
    La estrategia puede usar `self.exchange` para enviar órdenes.
    """
    exchange: Any
    symbol: str
    interval: str

    def on_start(self) -> None: ...
    def on_bar(self, bar: Bar) -> None: ...
    def on_finish(self) -> None: ...


class BaseStrategy:
    """Implementación base no-op para conveniencia."""
    def __init__(self, exchange: Any, symbol: str, interval: str, **params: Any) -> None:
        self.exchange = exchange
        self.symbol = symbol
        self.interval = interval
        self.params = params

    def on_start(self) -> None:
        pass

    def on_bar(self, bar: Bar) -> None:
        pass

    def on_finish(self) -> None:
        pass
