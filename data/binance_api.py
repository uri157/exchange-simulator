from __future__ import annotations

import time
from typing import Optional, List, Dict, Any

import requests

BASE_URL_FAPI = "https://fapi.binance.com"

# Interval -> milliseconds
_INTERVAL_TO_MS: Dict[str, int] = {
    "1m": 60_000,
    "3m": 3 * 60_000,
    "5m": 5 * 60_000,
    "15m": 15 * 60_000,
    "30m": 30 * 60_000,
    "1h": 60 * 60_000,
    "2h": 2 * 60 * 60_000,
    "4h": 4 * 60 * 60_000,
    "6h": 6 * 60 * 60_000,
    "8h": 8 * 60 * 60_000,
    "12h": 12 * 60 * 60_000,
    "1d": 24 * 60 * 60_000,
    "3d": 3 * 24 * 60 * 60_000,
    "1w": 7 * 24 * 60 * 60_000,
    "1M": 30 * 24 * 60 * 60_000,
}

_HEADERS = {"User-Agent": "sim-backtester/0.1"}


def get_klines(
    symbol: str,
    interval: str,
    startTime: Optional[int] = None,
    endTime: Optional[int] = None,
    limit: Optional[int] = None,
) -> List[List[Any]]:
    """
    Fetch historical klines from the Binance USD-M Futures API.

    Returns a list of kline arrays in the same shape as the API:
    [openTime, open, high, low, close, volume, closeTime, ...]
    """
    url = f"{BASE_URL_FAPI}/fapi/v1/klines"
    results: List[List[Any]] = []

    max_page = 1500
    page_limit = min(limit or max_page, max_page)

    params: Dict[str, Any] = {
        "symbol": symbol,
        "interval": interval,
        "limit": page_limit,
    }

    cur_start = startTime
    while True:
        if cur_start is not None:
            params["startTime"] = cur_start
        else:
            params.pop("startTime", None)

        if endTime is not None:
            params["endTime"] = endTime
        else:
            params.pop("endTime", None)

        resp = requests.get(url, params=params, headers=_HEADERS, timeout=15)
        if resp.status_code != 200:
            raise RuntimeError(f"Error fetching klines: {resp.status_code} - {resp.text}")

        data = resp.json()
        if not isinstance(data, list):
            raise RuntimeError(f"Unexpected klines response: {data}")

        if not data:
            break

        results.extend(data)

        # Stop if page was not full (end reached)
        if len(data) < page_limit:
            break

        # Stop if we've fulfilled requested limit (when caller gave a small limit)
        if limit is not None and len(results) >= limit:
            break

        # Advance cursor to the next bar after the last openTime we received
        last_open = int(data[-1][0])
        step = _INTERVAL_TO_MS.get(interval, 0) or 1
        next_start = last_open + step

        if endTime is not None and next_start > endTime:
            break

        # Safety: avoid infinite loops
        if cur_start is not None and next_start <= cur_start:
            break

        cur_start = next_start

        # Be nice to the API
        time.sleep(0.25)

    if limit is not None and len(results) > limit:
        results = results[:limit]

    return results


def get_funding_rates(
    symbol: str,
    startTime: Optional[int] = None,
    endTime: Optional[int] = None,
) -> List[Dict[str, Any]]:
    """
    Fetch historical funding rates from the Binance USD-M Futures API.

    Returns a list of dicts with at least:
    {"symbol": "...", "fundingTime": <ms>, "fundingRate": "..."}
    """
    url = f"{BASE_URL_FAPI}/fapi/v1/fundingRate"
    results: List[Dict[str, Any]] = []

    max_page = 1000
    params: Dict[str, Any] = {"symbol": symbol, "limit": max_page}

    cur_start = startTime
    while True:
        if cur_start is not None:
            params["startTime"] = cur_start
        else:
            params.pop("startTime", None)

        if endTime is not None:
            params["endTime"] = endTime
        else:
            params.pop("endTime", None)

        resp = requests.get(url, params=params, headers=_HEADERS, timeout=15)
        if resp.status_code != 200:
            raise RuntimeError(f"Error fetching funding rates: {resp.status_code} - {resp.text}")

        data = resp.json()
        if not isinstance(data, list):
            raise RuntimeError(f"Unexpected funding response: {data}")

        if not data:
            break

        results.extend(data)

        if len(data) < params.get("limit", max_page):
            break

        last_time = int(data[-1]["fundingTime"])
        next_start = last_time + 1

        if endTime is not None and next_start > endTime:
            break

        if cur_start is not None and next_start <= cur_start:
            break

        cur_start = next_start
        time.sleep(0.25)

    if endTime is not None:
        results = [d for d in results if int(d["fundingTime"]) <= endTime]

    return results
