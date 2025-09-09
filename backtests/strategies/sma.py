from __future__ import annotations
from collections import deque
from typing import Deque
from backtests.strategy_api import BaseStrategy
from sim.models import Bar


class SMA(BaseStrategy):
    """
    Estrategia de ejemplo (solo para pruebas del pipeline):
    - Cruce SMA rápida vs. lenta
    - Envia órdenes MARKET para abrir/cerrar
    Parámetros:
      fast: int (default 5)
      slow: int (default 20)
      qty: float (default 0.001)  # cantidad base a operar
    """
    def __init__(self, exchange, symbol: str, interval: str, **params):
        super().__init__(exchange, symbol, interval, **params)
        self.fast = int(params.get("fast", 5))
        self.slow = int(params.get("slow", 20))
        self.qty = float(params.get("qty", 0.001))
        self.closes: Deque[float] = deque(maxlen=max(self.fast, self.slow))
        self.last_fast = None
        self.last_slow = None

    def on_bar(self, bar: Bar) -> None:
        # Para órdenes MARKET inmediatas, proveemos precio "actual"
        self.exchange.last_price[self.symbol] = bar.open

        self.closes.append(bar.close)
        if len(self.closes) < self.slow:
            # aún calentando buffers
            if self.last_fast is None:
                self.last_fast = bar.close
                self.last_slow = bar.close
            return

        fast = sum(list(self.closes)[-self.fast:]) / self.fast
        slow = sum(list(self.closes)[-self.slow:]) / self.slow

        cross_up = self.last_fast is not None and self.last_slow is not None and self.last_fast <= self.last_slow and fast > slow
        cross_dn = self.last_fast is not None and self.last_slow is not None and self.last_fast >= self.last_slow and fast < slow

        pos_info = self.exchange.position_risk(self.symbol)
        pos_qty = float(pos_info.get("positionAmt", 0.0)) if isinstance(pos_info, dict) else 0.0

        if cross_up and pos_qty <= 0:
            buy_qty = abs(pos_qty) + self.qty
            self.exchange.new_order(self.symbol, "BUY", "MARKET", quantity=buy_qty)

        if cross_dn and pos_qty >= 0:
            sell_qty = abs(pos_qty) + self.qty
            self.exchange.new_order(self.symbol, "SELL", "MARKET", quantity=sell_qty)

        self.last_fast, self.last_slow = fast, slow
