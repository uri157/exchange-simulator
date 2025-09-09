import math
from sim.fill_models import OHLCPathFill
from sim.models import Bar, Side, Order, OrderType

def test_ohlc_path_fill_up_first():
    bar = Bar(open_time=0, open=100, high=120, low=80, close=110, volume=0, close_time=60_000)
    model = OHLCPathFill(up_first=True, slippage_bps=0.0)
    # Buy limit at 90 -> fill at 90 (maker)
    order = Order(order_id=1, symbol="TEST", side=Side.BUY, type=OrderType.LIMIT, quantity=1.0, price=90.0)
    fills = model.fills_on_bar(bar, order)
    assert fills and math.isclose(fills[0].price, 90.0, rel_tol=1e-6) and fills[0].is_maker is True
    # Sell stop-market at 90 -> trigger and fill at 90 (taker)
    order2 = Order(order_id=2, symbol="TEST", side=Side.SELL, type=OrderType.STOP_MARKET, quantity=1.0, stop_price=90.0)
    fills2 = model.fills_on_bar(bar, order2)
    assert fills2 and math.isclose(fills2[0].price, 90.0, rel_tol=1e-6) and fills2[0].is_maker is False
    # Buy stop-limit: stop=115, limit=110 -> triggers on way up, fills at 110 on way down
    order3 = Order(order_id=3, symbol="TEST", side=Side.BUY, type=OrderType.STOP_LIMIT, quantity=1.0, price=110.0, stop_price=115.0)
    fills3 = model.fills_on_bar(bar, order3)
    assert fills3 and math.isclose(fills3[0].price, 110.0, rel_tol=1e-6) and fills3[0].is_maker is True
    # Sell limit 130 (never reached) -> no fill
    order4 = Order(order_id=4, symbol="TEST", side=Side.SELL, type=OrderType.LIMIT, quantity=1.0, price=130.0)
    fills4 = model.fills_on_bar(bar, order4)
    assert fills4 == []

def test_ohlc_path_fill_down_first():
    bar = Bar(open_time=0, open=100, high=120, low=80, close=110, volume=0, close_time=60_000)
    model = OHLCPathFill(up_first=False, slippage_bps=0.0)
    # Sell limit at 110 -> fill at 110
    order = Order(order_id=1, symbol="TEST", side=Side.SELL, type=OrderType.LIMIT, quantity=1.0, price=110.0)
    fills = model.fills_on_bar(bar, order)
    assert fills and math.isclose(fills[0].price, 110.0, rel_tol=1e-6) and fills[0].is_maker is True
    # Sell stop-limit: stop=85, limit=90 -> triggers on way down, limit 90 remains unfilled in same bar
    order2 = Order(order_id=2, symbol="TEST", side=Side.SELL, type=OrderType.STOP_LIMIT, quantity=1.0, price=90.0, stop_price=85.0)
    fills2 = model.fills_on_bar(bar, order2)
    assert fills2 == [] and order2.type == OrderType.LIMIT