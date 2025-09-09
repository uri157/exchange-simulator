from sim.exchange_sim import SimExchange
from sim.fill_models import OHLCPathFill

def test_simple_profit():
    sim = SimExchange(starting_balance=10000.0, maker_fee_bps=0.0, taker_fee_bps=0.0, fill_model=OHLCPathFill(up_first=True))
    sim.last_price["TEST"] = 100.0
    sim.new_order("TEST", side="BUY", type="MARKET", quantity=1.0)
    pos = sim.positions.get("TEST")
    assert pos and abs(pos.quantity - 1.0) < 1e-9
    sim.last_price["TEST"] = 110.0
    sim.new_order("TEST", side="SELL", type="MARKET", quantity=1.0)
    pos = sim.positions.get("TEST")
    assert pos.quantity == 0
    assert abs(pos.realized_pnl - 10.0) < 1e-9
    assert abs(sim.account.balance - 10010.0) < 1e-9

def test_reduce_only_partial():
    sim = SimExchange(starting_balance=10000.0, maker_fee_bps=0.0, taker_fee_bps=0.0, fill_model=OHLCPathFill(up_first=True))
    sim.last_price["TEST"] = 50.0
    sim.new_order("TEST", side="BUY", type="MARKET", quantity=2.0)
    pos = sim.positions.get("TEST")
    assert pos and pos.quantity == 2.0
    sim.new_order("TEST", side="SELL", type="LIMIT", price=60.0, quantity=5.0, reduceOnly=True)
    from sim.models import Bar
    bar = Bar(open_time=0, open=50.0, high=60.0, low=50.0, close=60.0, volume=0.0, close_time=60000)
    sim.process_bar(bar)
    pos = sim.positions.get("TEST")
    assert pos.quantity == 0
    assert sim.get_open_orders("TEST") == []
    assert abs(pos.realized_pnl - 20.0) < 1e-9
    assert abs(sim.account.balance - 10020.0) < 1e-9