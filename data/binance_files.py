# data/binance_files.py
from __future__ import annotations

import csv
import glob
import os
from typing import Any, Dict, List, Optional


class BinanceFileData:
    """
    Loader de datos locales (CSV) con soporte para múltiples patrones de filename.

    Estructuras aceptadas de KLINES (todas con coma, con/sin header):
      - data/files/klines/{SYMBOL}_{INTERVAL}.csv
      - data/files/klines/{SYMBOL}/{INTERVAL}.csv
      - data/files/klines/{SYMBOL}/{INTERVAL}_{START}_{END}.csv

    Cada fila debe contener al menos:
      openTime, open, high, low, close, volume, closeTime, ...
      (los campos adicionales se ignoran)

    Para FUNDING:
      - data/files/funding/{SYMBOL}.csv
      - data/files/funding/{SYMBOL}_{START}_{END}.csv

    CSV de funding debe tener columnas 'fundingTime' y 'fundingRate'
    (en cualquier orden). Si no hay header y solo 2 columnas, se asumen
    [fundingTime, fundingRate].
    """

    def __init__(self, base_dir: str = "data/files"):
        self.base_dir = base_dir
        self.klines_dir = os.path.join(base_dir, "klines")
        self.funding_dir = os.path.join(base_dir, "funding")

    # --------------------------- utilidades ---------------------------

    def _first_existing(self, patterns: List[str]) -> Optional[str]:
        for pat in patterns:
            matches = sorted(glob.glob(pat))
            if matches:
                # si hay varios, tomar el más reciente por mtime
                matches.sort(key=lambda p: os.path.getmtime(p))
                return matches[-1]
        return None

    def _iter_csv_rows(self, path: str):
        with open(path, "r", newline="") as f:
            r = csv.reader(f)
            for row in r:
                if not row:
                    continue
                yield row

    # ---------------------------- klines -----------------------------

    def _find_klines_file(self, symbol: str, interval: str) -> Optional[str]:
        sym = symbol.upper()
        # patrones soportados (en orden de preferencia)
        patterns = [
            os.path.join(self.klines_dir, f"{sym}_{interval}.csv"),
            os.path.join(self.klines_dir, sym, f"{interval}.csv"),
            os.path.join(self.klines_dir, sym, f"{interval}_*.csv"),
        ]
        return self._first_existing(patterns)

    def get_klines(
        self,
        symbol: str,
        interval: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
        limit: Optional[int] = None,
    ) -> List[List[float]]:
        path = self._find_klines_file(symbol, interval)
        if not path or not os.path.exists(path):
            return []

        out: List[List[float]] = []
        rows = self._iter_csv_rows(path)

        # detectar header: si primer campo no es entero -> hay header
        first = None
        try:
            first = next(rows)
        except StopIteration:
            return []

        def _is_int(s: str) -> bool:
            try:
                int(s)
                return True
            except Exception:
                return False

        if not _is_int(first[0]):
            # es header, tomamos siguiente como primera data row
            try:
                first = next(rows)
            except StopIteration:
                return []

        def _parse_row(row: List[str]) -> Optional[List[float]]:
            if len(row) < 6:
                return None
            # algunas exportaciones no traen closeTime -> lo derivamos del openTime si falta
            open_time = int(float(row[0]))
            open_ = float(row[1])
            high = float(row[2])
            low = float(row[3])
            close = float(row[4])
            volume = float(row[5])
            close_time = int(float(row[6])) if len(row) > 6 and row[6] != "" else open_time
            return [open_time, open_, high, low, close, volume, close_time]

        # procesar primera fila ya leída
        parsed = _parse_row(first)
        if parsed is not None:
            out.append(parsed)

        # procesar el resto
        for row in rows:
            pr = _parse_row(row)
            if pr is None:
                continue
            out.append(pr)

        # filtrar por tiempo si corresponde (Binance usa [start, end], pero acá usamos inclusivo en ambos)
        if startTime is not None or endTime is not None:
            st = startTime if startTime is not None else -10**30
            et = endTime if endTime is not None else 10**30
            out = [r for r in out if st <= int(r[0]) <= et]

        # aplicar limit si corresponde
        if limit is not None and limit > 0:
            out = out[:limit]

        return out

    # --------------------------- funding -----------------------------

    def _find_funding_file(self, symbol: str) -> Optional[str]:
        sym = symbol.upper()
        patterns = [
            os.path.join(self.funding_dir, f"{sym}.csv"),
            os.path.join(self.funding_dir, f"{sym}_*.csv"),
        ]
        return self._first_existing(patterns)

    def get_funding_rates(
        self,
        symbol: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
    ) -> List[Dict[str, Any]]:
        path = self._find_funding_file(symbol)
        if not path or not os.path.exists(path):
            return []

        rows = list(self._iter_csv_rows(path))
        if not rows:
            return []

        # detectar header y columnas
        header = rows[0]
        has_header = False
        col_time = 0
        col_rate = 1

        def _is_int(s: str) -> bool:
            try:
                int(float(s))
                return True
            except Exception:
                return False

        if not _is_int(header[0]):
            has_header = True
            # intentar encontrar columnas por nombre
            lower = [c.strip().lower() for c in header]
            if "fundingtime" in lower:
                col_time = lower.index("fundingtime")
            if "fundingrate" in lower:
                col_rate = lower.index("fundingrate")
            data_rows = rows[1:]
        else:
            data_rows = rows

        out: List[Dict[str, Any]] = []
        for r in data_rows:
            if len(r) <= max(col_time, col_rate):
                continue
            t = int(float(r[col_time]))
            rate = float(r[col_rate])
            out.append({"fundingTime": t, "fundingRate": rate})

        # filtrar por tiempo
        if startTime is not None or endTime is not None:
            st = startTime if startTime is not None else -10**30
            et = endTime if endTime is not None else 10**30
            out = [x for x in out if st <= int(x["fundingTime"]) <= et]

        return out
