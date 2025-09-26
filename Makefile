PY?=python3
BASE_URL?=http://localhost:3001
WS_BASE?=ws://localhost:3001
SESSION?=
SYMBOLS?=BTCUSDT,ETHBTC
INTERVAL?=1m
RANGE_MINUTES?=120
SPEED?=1.0

export BASE_URL WS_BASE SESSION SYMBOLS INTERVAL RANGE_MINUTES SPEED

e2e:
$(PY) scripts/e2e_binance.py

ws-single:
$(PY) ws_log.py --session "$$SESSION" --streams "ethbtc@kline_$(INTERVAL)" --mode binance --pretty

ws-combined:
$(PY) ws_log.py --session "$$SESSION" --streams "ethbtc@kline_$(INTERVAL),btcusdt@kline_$(INTERVAL)" --mode binance --pretty
