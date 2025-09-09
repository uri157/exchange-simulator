# sim/fill_models.py
from __future__ import annotations

from typing import List
import random

from .entities import Bar, Order, Fill
from .enums import Side, OrderType, OrderStatus


__all__ = ["BaseFillModel", "OHLCPathFill", "RandomOHLC", "BookTickerFill"]


class BaseFillModel:
    """Interfaz base: decide cómo se llenan órdenes en una barra."""
    def fills_on_bar(self, bar: Bar, order: Order) -> List[Fill]:
        raise NotImplementedError


class OHLCPathFill(BaseFillModel):
    """
    Simula trayectoria intrabar:
      - up_first=True:  Open -> High -> Low -> Close
      - up_first=False: Open -> Low  -> High -> Close
    Con slippage en bps aplicado de forma desfavorable al trader.
    """
    def __init__(self, up_first: bool = True, slippage_bps: float = 0.0):
        self.up_first = up_first
        self.slippage_bps = slippage_bps
        self._slip_frac = slippage_bps / 10000.0

    def _apply_slippage(self, price: float, side: Side, bar: Bar) -> float:
        if self._slip_frac == 0:
            return price
        adj = price * (1 + self._slip_frac) if side == Side.BUY else price * (1 - self._slip_frac)
        # clamp dentro del rango por sanidad
        return min(max(adj, bar.low), bar.high)

    def fills_on_bar(self, bar: Bar, order: Order) -> List[Fill]:
        fills: List[Fill] = []

        if order.status in (OrderStatus.FILLED, OrderStatus.CANCELED, OrderStatus.EXPIRED):
            return fills

        side = order.side

        # 1) Chequeos al open (gaps/market/marketable limit)
        if order.type == OrderType.LIMIT:
            if side == Side.BUY and bar.open <= (order.price or 0):
                fills.append(Fill(
                    price=self._apply_slippage(bar.open, side, bar),
                    quantity=order.quantity, is_maker=False, ts_ms=bar.open_time
                ))
                return fills
            if side == Side.SELL and bar.open >= (order.price or 0):
                fills.append(Fill(
                    price=self._apply_slippage(bar.open, side, bar),
                    quantity=order.quantity, is_maker=False, ts_ms=bar.open_time
                ))
                return fills

        if order.type == OrderType.MARKET:
            fills.append(Fill(
                price=self._apply_slippage(bar.open, side, bar),
                quantity=order.quantity, is_maker=False, ts_ms=bar.open_time
            ))
            return fills

        if order.type == OrderType.STOP_MARKET:
            if side == Side.BUY and bar.open >= (order.stop_price or 0):
                fills.append(Fill(
                    price=self._apply_slippage(bar.open, side, bar),
                    quantity=order.quantity, is_maker=False, ts_ms=bar.open_time
                ))
                return fills
            if side == Side.SELL and bar.open <= (order.stop_price or 0):
                fills.append(Fill(
                    price=self._apply_slippage(bar.open, side, bar),
                    quantity=order.quantity, is_maker=False, ts_ms=bar.open_time
                ))
                return fills

        if order.type == OrderType.STOP_LIMIT:
            if side == Side.BUY and bar.open >= (order.stop_price or 0):
                if order.price is not None and bar.open <= order.price:
                    fills.append(Fill(
                        price=self._apply_slippage(bar.open, side, bar),
                        quantity=order.quantity, is_maker=False, ts_ms=bar.open_time
                    ))
                    return fills
                else:
                    order.type = OrderType.LIMIT
                    order.stop_price = None
            if side == Side.SELL and bar.open <= (order.stop_price or 0):
                if order.price is not None and bar.open >= order.price:
                    fills.append(Fill(
                        price=self._apply_slippage(bar.open, side, bar),
                        quantity=order.quantity, is_maker=False, ts_ms=bar.open_time
                    ))
                    return fills
                else:
                    order.type = OrderType.LIMIT
                    order.stop_price = None

        # 2) Ruta intrabar (timestamps intrabar aproximados en tercios)
        t1 = bar.open_time + (bar.close_time - bar.open_time) // 3
        t2 = bar.open_time + 2 * (bar.close_time - bar.open_time) // 3

        if self.up_first:
            # Open -> High
            if order.type == OrderType.LIMIT and side == Side.SELL and (order.price is not None) and bar.high >= order.price:
                fills.append(Fill(
                    price=self._apply_slippage(order.price, side, bar),
                    quantity=order.quantity, is_maker=True, ts_ms=t1
                ))
                return fills
            if order.type == OrderType.STOP_MARKET and side == Side.BUY and (order.stop_price is not None) and bar.high >= order.stop_price:
                fills.append(Fill(
                    price=self._apply_slippage(order.stop_price or bar.high, side, bar),
                    quantity=order.quantity, is_maker=False, ts_ms=t1
                ))
                return fills
            if order.type == OrderType.STOP_LIMIT and side == Side.BUY and (order.stop_price is not None) and bar.high >= order.stop_price:
                order.type = OrderType.LIMIT
                order.stop_price = None

            # High -> Low
            if order.type == OrderType.LIMIT and side == Side.BUY and (order.price is not None) and bar.low <= order.price:
                fills.append(Fill(
                    price=self._apply_slippage(order.price, side, bar),
                    quantity=order.quantity, is_maker=True, ts_ms=t2
                ))
                return fills
            if order.type == OrderType.STOP_MARKET and side == Side.SELL and (order.stop_price is not None) and bar.low <= order.stop_price:
                fills.append(Fill(
                    price=self._apply_slippage(order.stop_price or bar.low, side, bar),
                    quantity=order.quantity, is_maker=False, ts_ms=t2
                ))
                return fills
            if order.type == OrderType.STOP_LIMIT and side == Side.SELL and (order.stop_price is not None) and bar.low <= order.stop_price:
                order.type = OrderType.LIMIT
                order.stop_price = None

            # Low -> Close (sin nuevos extremos)
            if order.type == OrderType.LIMIT:
                if side == Side.BUY and bar.low <= (order.price or 0) <= bar.close:
                    fills.append(Fill(
                        price=self._apply_slippage(order.price or bar.close, side, bar),
                        quantity=order.quantity, is_maker=True, ts_ms=bar.close_time
                    ))
                    return fills
                if side == Side.SELL and bar.high >= (order.price or 0) >= bar.close:
                    fills.append(Fill(
                        price=self._apply_slippage(order.price or bar.close, side, bar),
                        quantity=order.quantity, is_maker=True, ts_ms=bar.close_time
                    ))
                    return fills
        else:
            # Open -> Low
            if order.type == OrderType.LIMIT and side == Side.BUY and (order.price is not None) and bar.low <= order.price:
                fills.append(Fill(
                    price=self._apply_slippage(order.price, side, bar),
                    quantity=order.quantity, is_maker=True, ts_ms=t1
                ))
                return fills
            if order.type == OrderType.STOP_MARKET and side == Side.SELL and (order.stop_price is not None) and bar.low <= order.stop_price:
                fills.append(Fill(
                    price=self._apply_slippage(order.stop_price or bar.low, side, bar),
                    quantity=order.quantity, is_maker=False, ts_ms=t1
                ))
                return fills
            if order.type == OrderType.STOP_LIMIT and side == Side.SELL and (order.stop_price is not None) and bar.low <= order.stop_price:
                order.type = OrderType.LIMIT
                order.stop_price = None

            # Low -> High
            if order.type == OrderType.LIMIT and side == Side.SELL and (order.price is not None) and bar.high >= order.price:
                fills.append(Fill(
                    price=self._apply_slippage(order.price, side, bar),
                    quantity=order.quantity, is_maker=True, ts_ms=t2
                ))
                return fills
            if order.type == OrderType.STOP_MARKET and side == Side.BUY and (order.stop_price is not None) and bar.high >= order.stop_price:
                fills.append(Fill(
                    price=self._apply_slippage(order.stop_price or bar.high, side, bar),
                    quantity=order.quantity, is_maker=False, ts_ms=t2
                ))
                return fills
            if order.type == OrderType.STOP_LIMIT and side == Side.BUY and (order.stop_price is not None) and bar.high >= order.stop_price:
                order.type = OrderType.LIMIT
                order.stop_price = None

            # High -> Close
            if order.type == OrderType.LIMIT:
                if side == Side.BUY and bar.low <= (order.price or 0) <= bar.close:
                    fills.append(Fill(
                        price=self._apply_slippage(order.price or bar.close, side, bar),
                        quantity=order.quantity, is_maker=True, ts_ms=bar.close_time
                    ))
                    return fills
                if side == Side.SELL and bar.high >= (order.price or 0) >= bar.close:
                    fills.append(Fill(
                        price=self._apply_slippage(order.price or bar.close, side, bar),
                        quantity=order.quantity, is_maker=True, ts_ms=bar.close_time
                    ))
                    return fills

        return fills


class RandomOHLC(BaseFillModel):
    """Elige up-first o down-first aleatoriamente (reproducible con semilla)."""
    def __init__(self, seed: int = 0, slippage_bps: float = 0.0):
        self.random = random.Random(seed)
        self.slippage_bps = slippage_bps

    def fills_on_bar(self, bar: Bar, order: Order) -> List[Fill]:
        up_first = bool(self.random.getrandbits(1))
        model = OHLCPathFill(up_first=up_first, slippage_bps=self.slippage_bps)
        return model.fills_on_bar(bar, order)


class BookTickerFill(BaseFillModel):
    """
    Modelo con spread L1 simulado.
    Si no hay book real, aplica half-spread al precio 'taker'.
    """
    def __init__(self, spread_bps: float = 2.0):
        self.spread_bps = spread_bps
        self.half_spread_frac = (spread_bps / 10000.0) / 2.0

    def _taker_price(self, price: float, side: Side) -> float:
        return price * (1 + self.half_spread_frac) if side == Side.BUY else price * (1 - self.half_spread_frac)

    def fills_on_bar(self, bar: Bar, order: Order) -> List[Fill]:
        fills: List[Fill] = []
        side = order.side
        px = bar.open

        if order.type == OrderType.LIMIT:
            if side == Side.BUY and px <= (order.price or 0):
                fills.append(Fill(price=self._taker_price(px, side), quantity=order.quantity, is_maker=False, ts_ms=bar.open_time))
                return fills
            if side == Side.SELL and px >= (order.price or 0):
                fills.append(Fill(price=self._taker_price(px, side), quantity=order.quantity, is_maker=False, ts_ms=bar.open_time))
                return fills

        if order.type == OrderType.MARKET:
            fills.append(Fill(price=self._taker_price(px, side), quantity=order.quantity, is_maker=False, ts_ms=bar.open_time))
            return fills

        if order.type == OrderType.STOP_MARKET:
            if side == Side.BUY and px >= (order.stop_price or 0):
                fills.append(Fill(price=self._taker_price(px, side), quantity=order.quantity, is_maker=False, ts_ms=bar.open_time))
                return fills
            if side == Side.SELL and px <= (order.stop_price or 0):
                fills.append(Fill(price=self._taker_price(px, side), quantity=order.quantity, is_maker=False, ts_ms=bar.open_time))
                return fills

        if order.type == OrderType.STOP_LIMIT:
            if side == Side.BUY and px >= (order.stop_price or 0):
                if order.price is not None and px <= order.price:
                    fills.append(Fill(price=self._taker_price(px, side), quantity=order.quantity, is_maker=False, ts_ms=bar.open_time))
                    return fills
                else:
                    order.type = OrderType.LIMIT
                    order.stop_price = None
            if side == Side.SELL and px <= (order.stop_price or 0):
                if order.price is not None and px >= order.price:
                    fills.append(Fill(price=self._taker_price(px, side), quantity=order.quantity, is_maker=False, ts_ms=bar.open_time))
                    return fills
                else:
                    order.type = OrderType.LIMIT
                    order.stop_price = None

        # fallback: usar OHLC con ajuste de precio para tomador
        ohlc = OHLCPathFill(up_first=True, slippage_bps=0.0)
        fills = ohlc.fills_on_bar(bar, order)
        for f in fills:
            if not f.is_maker:
                f.price = self._taker_price(f.price, side)
        return fills
