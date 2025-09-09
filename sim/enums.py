from __future__ import annotations

from enum import Enum

__all__ = ["Side", "OrderType", "TimeInForce", "OrderStatus"]


class Side(Enum):
    """Order/position side."""
    BUY = "BUY"
    SELL = "SELL"


class OrderType(Enum):
    """Order type enumeration."""
    MARKET = "MARKET"
    LIMIT = "LIMIT"
    STOP_MARKET = "STOP_MARKET"
    STOP_LIMIT = "STOP_LIMIT"


class TimeInForce(Enum):
    """Time-in-force policies."""
    GTC = "GTC"   # Good-Til-Canceled
    IOC = "IOC"   # Immediate-Or-Cancel (not fully supported in sim)
    FOK = "FOK"   # Fill-Or-Kill (not supported in sim)


class OrderStatus(Enum):
    """Status of an order."""
    NEW = "NEW"
    PARTIALLY_FILLED = "PARTIALLY_FILLED"
    FILLED = "FILLED"
    CANCELED = "CANCELED"
    REJECTED = "REJECTED"
    EXPIRED = "EXPIRED"
