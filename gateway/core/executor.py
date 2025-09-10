"""
Executor: lógica de órdenes/fills/PnL del simulador.
- Soporta MARKET y LIMIT (GTC)
- Fees maker/taker (bps)
- Slippage simple para MARKET (en contra)
- Reduce-only (opcional)
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Dict, List, Optional, Tuple, Any

from .models import Account, Order, now_ms
from .store import RunStore


@dataclass
class Executor:
    account: Account
    store: RunStore
    maker_bps: float = 2.0
    taker_bps: float = 4.0
    slippage_bps: float = 0.0

    def __post_init__(self) -> None:
        # pasar a fracción
        self._maker = float(self.maker_bps) / 10_000.0
        self._taker = float(self.taker_bps) / 10_000.0
        self._slip = float(self.slippage_bps) / 10_000.0
        self.open_orders: Dict[int, Order] = {}
        self._next_order_id = 1

    # --------------- helpers internos ---------------
    def _apply_fill(self, ts: int, side: str, px: float, qty: float, taker: bool) -> Tuple[float, float, float]:
        """Aplica el fill a la cuenta. Devuelve (realized_pnl, fee, new_avg_price)."""
        notional = float(px) * float(qty)
        fee = notional * (self._taker if taker else self._maker)
        realized = 0.0
        pos = self.account.position

        if side == "BUY":
            # cerrar short si existe
            if pos.qty < 0:
                close_qty = min(qty, -pos.qty)
                realized += (pos.avg_price - px) * close_qty  # profit short si px < avg
                pos.qty += close_qty  # menos negativo
                qty -= close_qty
                # abrir/incrementar long con resto
                if qty > 0:
                    new_qty = pos.qty + qty
                    pos.avg_price = (pos.avg_price * pos.qty + px * qty) / new_qty if new_qty != 0 else px
                    pos.qty = new_qty
            else:
                # long add
                new_qty = pos.qty + qty
                pos.avg_price = (pos.avg_price * pos.qty + px * qty) / new_qty if new_qty != 0 else px
                pos.qty = new_qty
            self.account.cash -= (notional + fee)
        else:  # SELL
            if pos.qty > 0:
                close_qty = min(qty, pos.qty)
                realized += (px - pos.avg_price) * close_qty  # profit long si px > avg
                pos.qty -= close_qty
                qty -= close_qty
                if qty > 0:
                    # abrir/incrementar short
                    new_qty = pos.qty - qty
                    pos.avg_price = (pos.avg_price * pos.qty + px * (-qty)) / new_qty if new_qty != 0 else px
                    pos.qty = new_qty
            else:
                # short add
                new_qty = pos.qty - qty
                pos.avg_price = (pos.avg_price * pos.qty + px * (-qty)) / new_qty if new_qty != 0 else px
                pos.qty = new_qty
            self.account.cash += (notional - fee)

        return realized, fee, pos.avg_price

    # --------------- API del motor -------------------
    def place_order(
        self,
        symbol: str,
        side: str,
        type_: str,
        qty: float,
        price: Optional[float],
        tif: str,
        cur_price: float,
        client_id: Optional[str] = None,
        reduce_only: bool = False,
        stop_price: Optional[float] = None,
    ) -> Order:
        """Crea una orden y, si es MARKET, la llena al instante (taker)."""
        oid = self._next_order_id
        self._next_order_id += 1
        coid = client_id or f"sim-{oid}"
        order = Order(
            order_id=oid,
            client_order_id=coid,
            symbol=symbol,
            side=side,
            type=type_.upper(),
            qty=float(qty),
            price=float(price) if price is not None else None,
            tif=(tif or "GTC").upper(),
        )

        ts = now_ms()
        pos = self.account.position

        # Reduce-only: recortar qty para no aumentar exposición
        if reduce_only:
            if side.upper() == "BUY" and pos.qty >= 0:
                # No hay short a cerrar → cancelo inmediata
                order.status = "CANCELED"
                return order
            if side.upper() == "SELL" and pos.qty <= 0:
                order.status = "CANCELED"
                return order
            # clamp qty a la parte que realmente cierra
            if side.upper() == "BUY" and pos.qty < 0:
                order.qty = min(order.qty, -pos.qty)
            if side.upper() == "SELL" and pos.qty > 0:
                order.qty = min(order.qty, pos.qty)

        if order.type == "MARKET":
            # slippage en contra
            if side.upper() == "BUY":
                fill_px = float(cur_price) * (1.0 + self._slip)
            else:
                fill_px = float(cur_price) * (1.0 - self._slip)
            realized, fee, _ = self._apply_fill(ts, side.upper(), fill_px, order.qty, taker=True)
            order.executed_qty = order.qty
            order.status = "FILLED"
            order.is_maker = False
            order.fills.append({
                "price": f"{fill_px:.8f}",
                "qty": f"{order.qty:.8f}",
                "commission": f"{fee:.8f}",
                "commissionAsset": "USDT",
            })
            self.store.log_fill(ts, symbol, side.upper(), fill_px, order.qty, realized, fee, is_maker=False)
            return order

        # LIMIT: queda en libro hasta cruce por OHLC (modelo maker)
        self.open_orders[oid] = order
        return order

    def on_bar(self, bar_open: float, bar_high: float, bar_low: float, ts_close: int, symbol: str) -> None:
        """Intenta cruzar LIMITs con la barra actual (price-time priority simple)."""
        to_fill: List[int] = []
        for oid, o in self.open_orders.items():
            if o.status != "NEW" or o.type != "LIMIT" or o.price is None:
                continue
            # BUY se llena si low <= price; SELL si high >= price
            if o.side.upper() == "BUY" and bar_low <= o.price:
                fill_px = o.price
            elif o.side.upper() == "SELL" and bar_high >= o.price:
                fill_px = o.price
            else:
                continue

            realized, fee, _ = self._apply_fill(ts_close, o.side.upper(), fill_px, o.qty, taker=False)
            o.executed_qty = o.qty
            o.status = "FILLED"
            o.is_maker = True
            o.fills.append({
                "price": f"{fill_px:.8f}",
                "qty": f"{o.qty:.8f}",
                "commission": f"{fee:.8f}",
                "commissionAsset": "USDT",
            })
            self.store.log_fill(ts_close, symbol, o.side.upper(), fill_px, o.qty, realized, fee, is_maker=True)
            to_fill.append(oid)

        for oid in to_fill:
            self.open_orders.pop(oid, None)

    def cancel(self, order_id: int) -> bool:
        o = self.open_orders.get(order_id)
        if not o:
            return False
        o.status = "CANCELED"
        self.open_orders.pop(order_id, None)
        return True
