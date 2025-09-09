# sim/entities.py
from __future__ import annotations

from dataclasses import dataclass
from typing import Optional

from .enums import OrderStatus, OrderType, Side, TimeInForce


EPS = 1e-12  # tolerancia numérica para comparaciones con 0.0


@dataclass
class Order:
    """Order model used by the simulator."""
    order_id: int
    symbol: str
    side: Side
    type: OrderType
    quantity: float
    price: Optional[float] = None        # for LIMIT or STOP_LIMIT
    stop_price: Optional[float] = None   # for STOP_* orders
    time_in_force: TimeInForce = TimeInForce.GTC
    reduce_only: bool = False
    client_order_id: Optional[str] = None

    status: OrderStatus = OrderStatus.NEW
    filled_quantity: float = 0.0         # accumulated filled qty
    avg_fill_price: float = 0.0          # VWAP of fills

    def remaining_qty(self) -> float:
        """Quantity not executed yet."""
        return self.quantity - self.filled_quantity


@dataclass
class Fill:
    """Execution fill information (optionally timestamped intrabar)."""
    price: float
    quantity: float
    is_maker: bool
    fee_paid: float = 0.0
    ts_ms: Optional[int] = None          # timestamp dentro de la vela (si aplica)


@dataclass
class Position:
    """
    Net position (one-way). Positive = long, negative = short.
    `entry_price` is the weighted-average entry of the *current* open quantity.
    """
    symbol: str
    quantity: float = 0.0
    entry_price: float = 0.0
    realized_pnl: float = 0.0

    def reset(self) -> None:
        self.quantity = 0.0
        self.entry_price = 0.0
        self.realized_pnl = 0.0

    def update(self, qty_change: float, price: float) -> float:
        """
        Apply a trade to the position and return *realized* PnL from this update.

        - If increasing same-side exposure -> no realized PnL; update VWAP.
        - If reducing/closing -> realize PnL on the closed portion.
        - If flipping -> realize PnL on the closed portion and open new pos at `price`.
        """
        # No existing position -> open fresh
        if abs(self.quantity) < EPS:
            self.quantity = qty_change
            self.entry_price = price
            return 0.0

        # Same direction (increase exposure)
        if self.quantity * qty_change > 0:
            new_qty = self.quantity + qty_change
            # Weighted-average entry price
            self.entry_price = (
                self.entry_price * abs(self.quantity) + price * abs(qty_change)
            ) / abs(new_qty)
            self.quantity = new_qty
            return 0.0

        # Opposite direction -> reduce / close / flip
        if abs(qty_change) < abs(self.quantity) - EPS:
            # Partial reduction
            closed_qty = abs(qty_change)
            pnl = (
                (price - self.entry_price) * closed_qty
                if self.quantity > 0
                else (self.entry_price - price) * closed_qty
            )
            self.realized_pnl += pnl
            self.quantity += qty_change  # move towards zero
            if abs(self.quantity) < EPS:
                self.quantity = 0.0
                self.entry_price = 0.0
            return pnl

        # Close completely and possibly flip
        closed_qty = abs(self.quantity)
        pnl = (
            (price - self.entry_price) * closed_qty
            if self.quantity > 0
            else (self.entry_price - price) * closed_qty
        )
        self.realized_pnl += pnl

        # New open quantity after closing the old one
        new_open_qty = self.quantity + qty_change  # sign of new side
        if abs(new_open_qty) < EPS:
            self.quantity = 0.0
            self.entry_price = 0.0
        else:
            self.quantity = new_open_qty
            self.entry_price = price
        return pnl


@dataclass
class Account:
    """Account (simplified). `balance` excludes unrealized PnL."""
    balance: float                  # wallet balance (realized)
    starting_balance: float
    maker_fee: float = 0.0002       # 0.02% maker
    taker_fee: float = 0.0004       # 0.04% taker
    total_fees_paid: float = 0.0
    total_funding: float = 0.0      # positive => paid, negative => received

    def __init__(self, starting_balance: float, maker_fee: float = 0.0002, taker_fee: float = 0.0004):
        self.balance = starting_balance
        self.starting_balance = starting_balance
        self.maker_fee = maker_fee
        self.taker_fee = taker_fee
        self.total_fees_paid = 0.0
        self.total_funding = 0.0

    def apply_fee(self, fee: float) -> None:
        """Deduct a fee from the balance and accumulate totals."""
        self.balance -= fee
        self.total_fees_paid += fee

    def __repr__(self) -> str:
        realized = self.balance - self.starting_balance
        return f"Account(balance={self.balance:.2f}, realized_pnl={realized:.2f}, fees_paid={self.total_fees_paid:.2f})"


@dataclass
class Bar:
    """Market data bar (OHLCV). Times in milliseconds since epoch."""
    open_time: int
    open: float
    high: float
    low: float
    close: float
    volume: float
    close_time: int
    # Optional symbol to support multi-symbol simulations without adapters
    symbol: str = ""

    def __repr__(self) -> str:
        return f"Bar(time={self.open_time}, O={self.open}, H={self.high}, L={self.low}, C={self.close})"
