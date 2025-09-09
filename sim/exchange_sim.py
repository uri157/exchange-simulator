# sim/exchange_sim.py
from __future__ import annotations

from typing import Any, Dict, List, Optional

from sim.models import (
    Account,
    Bar,
    Order,
    OrderStatus,
    OrderType,
    Position,
    Side,
    TimeInForce,
)
from sim.fill_models import BaseFillModel, OHLCPathFill


class Exchange:
    """Interfaz tipo Binance que las estrategias esperan."""

    def new_order(
        self,
        symbol: str,
        side: str,
        type: str,
        quantity: float,
        price: Optional[float] = None,
        stopPrice: Optional[float] = None,
        timeInForce: str = "GTC",
        reduceOnly: bool = False,
        newClientOrderId: Optional[str] = None,
    ) -> Any:
        raise NotImplementedError

    def cancel_order(
        self,
        symbol: str,
        orderId: Optional[int] = None,
        origClientOrderId: Optional[str] = None,
    ) -> Any:
        raise NotImplementedError

    def get_open_orders(self, symbol: Optional[str] = None) -> List[Order]:
        raise NotImplementedError

    def position_risk(self, symbol: Optional[str] = None) -> Any:
        raise NotImplementedError

    def account_info(self) -> Any:
        raise NotImplementedError

    def get_klines(
        self,
        symbol: str,
        interval: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
        limit: Optional[int] = None,
    ) -> List[List[float]]:
        raise NotImplementedError

    def get_funding_rates(
        self,
        symbol: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
    ) -> List[Dict[str, Any]]:
        raise NotImplementedError

    def set_leverage(self, symbol: str, leverage: int) -> None:
        raise NotImplementedError

    def set_position_mode(self, hedge_mode: bool = False) -> None:
        raise NotImplementedError


class SimExchange(Exchange):
    """
    Exchange simulado para backtests / replay.

    - One-way por defecto (posición neta por símbolo). `hedge_mode` queda como hook para futuro.
    - Los fills se delegan a un `BaseFillModel` (por defecto OHLC up-first).
    """

    def __init__(
        self,
        starting_balance: float = 100_000.0,
        maker_fee_bps: float = 0.2,
        taker_fee_bps: float = 0.4,
        fill_model: Optional[BaseFillModel] = None,
        hedge_mode: bool = False,
        data_source: Optional[object] = None,
    ):
        # Fees en fracción
        maker_fee = maker_fee_bps / 10_000.0
        taker_fee = taker_fee_bps / 10_000.0

        self.account = Account(starting_balance, maker_fee=maker_fee, taker_fee=taker_fee)
        self.positions: Dict[str, Position] = {}
        self._open_orders: List[Order] = []
        self._order_id_seq = 1

        self.fill_model: BaseFillModel = fill_model if fill_model is not None else OHLCPathFill(up_first=True)
        self.hedge_mode = hedge_mode  # Hook (por ahora no cambia PnL)
        self.data_source = data_source

        self.last_price: Dict[str, float] = {}        # último precio conocido por símbolo
        self.trade_log: List[Dict[str, Any]] = []     # historial de fills
        self.leverage: Dict[str, int] = {}
        self._clock_ms: Optional[int] = None          # reloj interno (último close procesado)

    # ---------------------------------------------------------------------
    # Órdenes
    # ---------------------------------------------------------------------
    def new_order(
        self,
        symbol: str,
        side: str,
        type: str,
        quantity: float,
        price: Optional[float] = None,
        stopPrice: Optional[float] = None,
        timeInForce: str = "GTC",
        reduceOnly: bool = False,
        newClientOrderId: Optional[str] = None,
    ) -> Order:
        """Crea una nueva orden y la introduce al simulador."""
        side_enum = Side.BUY if str(side).upper() == "BUY" else Side.SELL
        type_u = str(type).upper()
        type_enum = OrderType[type_u] if type_u in OrderType.__members__ else OrderType.LIMIT
        tif_enum = TimeInForce[str(timeInForce).upper()] if str(timeInForce).upper() in TimeInForce.__members__ else TimeInForce.GTC

        order = Order(
            order_id=self._order_id_seq,
            symbol=symbol,
            side=side_enum,
            type=type_enum,
            quantity=float(quantity),
            price=float(price) if price is not None else None,
            stop_price=float(stopPrice) if stopPrice is not None else None,
            time_in_force=tif_enum,
            reduce_only=bool(reduceOnly),
            client_order_id=newClientOrderId or "",
        )
        self._order_id_seq += 1

        # MARKET -> ejecuta inmediato al último precio conocido
        if order.type == OrderType.MARKET:
            if symbol not in self.last_price:
                # No conocemos precio de mercado aún
                raise RuntimeError("No market price available for execution.")
            exec_price = float(self.last_price[symbol])

            # Ajuste reduce-only si aplica
            fill_qty = float(order.quantity)
            if order.reduce_only:
                pos = self.positions.get(symbol, Position(symbol))
                if pos.quantity == 0:
                    fill_qty = 0.0
                elif (pos.quantity > 0 and order.side == Side.BUY) or (pos.quantity < 0 and order.side == Side.SELL):
                    fill_qty = 0.0
                else:
                    fill_qty = min(fill_qty, abs(pos.quantity))
            if fill_qty == 0.0:
                # No llena nada: se deja NEW (o se podría REJECTED)
                order.status = OrderStatus.NEW
                self._open_orders.append(order)
                return order

            # Fee taker
            fee = exec_price * fill_qty * self.account.taker_fee

            # Actualiza posición
            pos = self.positions.get(symbol, Position(symbol))
            signed_qty = fill_qty if order.side == Side.BUY else -fill_qty
            realized = pos.update(signed_qty, exec_price)
            self.positions[symbol] = pos

            # Actualiza cuenta
            self.account.balance += realized
            self.account.apply_fee(fee)

            # Log
            self.trade_log.append(
                {
                    "timestamp": self._clock_ms,  # puede ser None si aún no procesamos ningún bar
                    "symbol": symbol,
                    "side": order.side.value,
                    "price": exec_price,
                    "quantity": fill_qty,
                    "realized_pnl": realized,
                    "fee": fee,
                    "is_maker": False,
                }
            )

            # Orden completada (posible fill parcial si reduce_only recortó)
            order.filled_quantity = fill_qty
            order.avg_fill_price = exec_price
            order.status = OrderStatus.FILLED if abs(fill_qty - order.quantity) < 1e-12 else OrderStatus.PARTIALLY_FILLED

            # Si quedó remanente (por reduce_only), mantener la orden viva por el resto
            if order.status == OrderStatus.PARTIALLY_FILLED:
                order.quantity = float(order.quantity)  # cantidad original
                self._open_orders.append(order)
            return order

        # STOP_MARKET/STOP_LIMIT/LIMIT -> quedan abiertas y se resuelven en `process_bar`
        order.status = OrderStatus.NEW
        self._open_orders.append(order)
        return order

    def cancel_order(
        self,
        symbol: str,
        orderId: Optional[int] = None,
        origClientOrderId: Optional[str] = None,
    ) -> Dict[str, Any]:
        """Cancela una orden abierta por `orderId` o `clientOrderId`."""
        target: Optional[Order] = None
        for o in list(self._open_orders):
            if o.symbol != symbol:
                continue
            if orderId is not None and o.order_id == orderId:
                target = o
                break
            if origClientOrderId and o.client_order_id == origClientOrderId:
                target = o
                break

        if not target:
            return {"status": "UNKNOWN_ORDER"}

        target.status = OrderStatus.CANCELED
        self._open_orders.remove(target)
        return {"status": "CANCELED", "orderId": target.order_id, "clientOrderId": target.client_order_id}

    def get_open_orders(self, symbol: Optional[str] = None) -> List[Order]:
        """Devuelve órdenes abiertas (filtradas por símbolo si se provee)."""
        if symbol:
            return [o for o in self._open_orders if o.symbol == symbol]
        return list(self._open_orders)

    # ---------------------------------------------------------------------
    # Estado de cuenta / posiciones
    # ---------------------------------------------------------------------
    def position_risk(self, symbol: Optional[str] = None) -> Any:
        """
        Devuelve la(s) posición(es) con PnL no realizado.
        Estructura: {"symbol", "positionAmt", "entryPrice", "unRealizedProfit"}
        """
        def _pnl(p: Position, last: float) -> float:
            if p.quantity == 0:
                return 0.0
            return (last - p.entry_price) * p.quantity

        if symbol:
            pos = self.positions.get(symbol, Position(symbol))
            last_price = self.last_price.get(symbol, pos.entry_price)
            return {
                "symbol": symbol,
                "positionAmt": float(pos.quantity),
                "entryPrice": float(pos.entry_price),
                "unRealizedProfit": float(_pnl(pos, last_price)),
            }

        out: List[Dict[str, Any]] = []
        for sym, pos in self.positions.items():
            last_price = self.last_price.get(sym, pos.entry_price)
            out.append(
                {
                    "symbol": sym,
                    "positionAmt": float(pos.quantity),
                    "entryPrice": float(pos.entry_price),
                    "unRealizedProfit": float(_pnl(pos, last_price)),
                }
            )
        return out

    def account_info(self) -> Dict[str, float]:
        """Devuelve balances y fees actuales del simulador."""
        total_unrealized = 0.0
        for sym, pos in self.positions.items():
            if pos.quantity != 0:
                price = self.last_price.get(sym, pos.entry_price)
                total_unrealized += (price - pos.entry_price) * pos.quantity
        return {
            "balance": float(self.account.balance),
            "unrealized_profit": float(total_unrealized),
            "equity": float(self.account.balance + total_unrealized),
            "maker_fee": float(self.account.maker_fee),
            "taker_fee": float(self.account.taker_fee),
        }

    # ---------------------------------------------------------------------
    # Data (REST / archivos)
    # ---------------------------------------------------------------------
    def get_klines(
        self,
        symbol: str,
        interval: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
        limit: Optional[int] = None,
    ) -> List[List[float]]:
        """Obtiene klines desde un DataSource o, por defecto, la API REST."""
        if self.data_source and hasattr(self.data_source, "get_klines"):
            return self.data_source.get_klines(symbol, interval, startTime, endTime, limit)
        try:
            from data import binance_api
        except ImportError as e:
            raise RuntimeError("Data source not available for klines.") from e
        return binance_api.get_klines(symbol, interval, startTime=startTime, endTime=endTime, limit=limit)

    def get_funding_rates(
        self,
        symbol: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
    ) -> List[Dict[str, Any]]:
        """Obtiene funding desde un DataSource o, por defecto, la API REST."""
        if self.data_source and hasattr(self.data_source, "get_funding_rates"):
            return self.data_source.get_funding_rates(symbol, startTime, endTime)
        try:
            from data import binance_api
        except ImportError as e:
            raise RuntimeError("Data source not available for funding rates.") from e
        return binance_api.get_funding_rates(symbol, startTime=startTime, endTime=endTime)

    # ---------------------------------------------------------------------
    # Config
    # ---------------------------------------------------------------------
    def set_leverage(self, symbol: str, leverage: int) -> None:
        """Guarda el apalancamiento elegido (no se utiliza en el PnL por ahora)."""
        self.leverage[symbol] = int(leverage)

    def set_position_mode(self, hedge_mode: bool = False) -> None:
        """Activa/desactiva hedge mode (hook; por ahora el PnL es one-way)."""
        self.hedge_mode = bool(hedge_mode)

    # ---------------------------------------------------------------------
    # Motor de simulación
    # ---------------------------------------------------------------------
    def process_bar(self, bar: Bar, funding_rate: Optional[float] = None) -> None:
        """
        Procesa un bar de mercado:
        - Actualiza `last_price`
        - Intenta ejecutar órdenes abiertas vía `fill_model`
        - Aplica funding al cierre del bar (si se provee)
        """
        symbol = getattr(bar, "symbol", "") or (
            self._open_orders[0].symbol if self._open_orders else (next(iter(self.positions)) if self.positions else "UNKNOWN")
        )

        # Reloj interno y precio de apertura del intervalo
        self._clock_ms = int(bar.open_time)
        self.last_price[symbol] = float(bar.open)

        # Intentar fills para cada orden abierta del símbolo
        for order in list(self._open_orders):
            if order.symbol != symbol:
                continue

            fills = self.fill_model.fills_on_bar(bar, order)
            if not fills:
                continue

            total_filled_qty = 0.0
            notional_accum = 0.0

            for fill in fills:
                fill_qty = float(fill.quantity)
                fill_price = float(fill.price)

                # En reduceOnly, nunca aumentar exposición (ni cambiar signo)
                if order.reduce_only:
                    pos = self.positions.get(symbol, Position(symbol))
                    if pos.quantity == 0:
                        fill_qty = 0.0
                    elif (pos.quantity > 0 and order.side == Side.BUY) or (pos.quantity < 0 and order.side == Side.SELL):
                        fill_qty = 0.0
                    elif abs(fill_qty) > abs(pos.quantity):
                        fill_qty = abs(pos.quantity)
                if fill_qty == 0.0:
                    continue

                signed_qty = fill_qty if order.side == Side.BUY else -fill_qty

                # Fee maker/taker según el fill
                fee_rate = self.account.maker_fee if fill.is_maker else self.account.taker_fee
                fee = fill_price * fill_qty * fee_rate

                # Actualiza posición
                pos = self.positions.get(symbol, Position(symbol))
                realized = pos.update(signed_qty, fill_price)
                self.positions[symbol] = pos

                # Actualiza cuenta
                self.account.balance += realized
                self.account.apply_fee(fee)

                # Timestamp del fill: si el modelo provee `ts_ms`, úsalo; si no, usar close del bar
                ts_ms = getattr(fill, "ts_ms", None)
                log_ts = int(ts_ms) if ts_ms is not None else int(bar.close_time)

                # Log del fill individual
                self.trade_log.append(
                    {
                        "timestamp": log_ts,
                        "symbol": symbol,
                        "side": order.side.value,
                        "price": fill_price,
                        "quantity": fill_qty,
                        "is_maker": bool(fill.is_maker),
                        "realized_pnl": realized,
                        "fee": fee,
                    }
                )

                total_filled_qty += fill_qty
                notional_accum += fill_price * fill_qty

            if total_filled_qty > 0.0:
                order.filled_quantity += total_filled_qty
                order.avg_fill_price = notional_accum / total_filled_qty

                remaining = max(order.quantity - order.filled_quantity, 0.0)
                if remaining <= 1e-12:
                    order.status = OrderStatus.FILLED
                    if order in self._open_orders:
                        self._open_orders.remove(order)
                else:
                    order.status = OrderStatus.PARTIALLY_FILLED
                    # mantener la orden viva con el remanente (usamos filled_quantity para el cálculo)

        # Funding al cierre del bar (si aplica)
        if funding_rate is not None:
            pos = self.positions.get(symbol)
            if pos and pos.quantity != 0:
                # Pago de funding aproximado: notional * rate
                payment = pos.quantity * funding_rate * float(bar.close)
                self.account.balance -= payment
                self.account.total_funding += payment
                pos.realized_pnl -= payment  # reflejar carry en realized

        # Precio de cierre del intervalo y avanzar reloj
        self.last_price[symbol] = float(bar.close)
        self._clock_ms = int(bar.close_time)

    def get_equity(self) -> float:
        """Equity = balance + PnL no realizado."""
        equity = float(self.account.balance)
        for sym, pos in self.positions.items():
            if pos.quantity != 0:
                price = self.last_price.get(sym, pos.entry_price)
                equity += (price - pos.entry_price) * pos.quantity
        return float(equity)
