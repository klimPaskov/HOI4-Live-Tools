"""Read-only proof of a completed own-country weekly technology scan."""

import math

from ..state.assemble import days_between


def _integer(value):
    return value if type(value) is int and value >= 0 else None


def _wall(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def closed_technology_scan(store, tag, generation, schema, catalog, day):
    """Return (IDs or None, proof metadata), without I/O or UI state inference.

    The caller must bind catalog and installed IDs to its trusted manifest and
    validate freshness against its own current clock observation. This function
    proves the log interval, own-country provenance and unchanged closing count.
    """
    def unknown(reason):
        return None, {"note": reason}

    if (not tag or store.player() != tag or not isinstance(day, dict)
            or store.generation != generation or store.schema != schema
            or day.get("g") != generation or day.get("v") != schema or day.get("t") != tag
            or not isinstance(catalog, list) or not catalog
            or any(not isinstance(tid, str) or not tid for tid in catalog)
            or len(set(catalog)) != len(catalog)):
        return unknown("technology catalog or own-country generation is unverified")
    hello = store.get("hello", tag)
    if (not isinstance(hello, dict) or hello.get("g") != generation or hello.get("v") != schema
            or hello.get("t") != tag or not _wall(hello.get("_wall"))):
        return unknown("own-country generation handshake is unverified")
    histories = getattr(store, "history", {})
    if any(other != tag and rows for other, rows in histories.items()):
        return unknown("technology stream contains another country")
    n, count = _integer(day.get("n")), _integer(day.get("techs"))
    hello_seq, day_seq = _integer(hello.get("_sequence")), _integer(day.get("_sequence"))
    if (n is None or count is None or not _wall(day.get("_wall")) or day["_wall"] < hello["_wall"]
            or hello_seq is None or day_seq is None or hello_seq >= day_seq):
        return unknown("technology count or timeline is unverified")
    if count == 0:
        return [], {"known_as_of_day": n}
    group = getattr(store, "groups", {}).get("kt")
    scan_day = _integer(group.get("n")) if isinstance(group, dict) else None
    if scan_day is None or not 0 < n - scan_day <= 8:
        return unknown("known-technology scan is incomplete or stale")
    anchors = [record for record in histories.get(tag, []) if record.get("n") == scan_day]
    preceding = [record for record in histories.get(tag, []) if record.get("n") == scan_day - 1]
    if len(anchors) != 1 or len(preceding) != 1:
        return unknown("known-technology scan has no unique own-country day interval")
    anchor = anchors[0]
    previous = preceding[0]
    for boundary in (previous, anchor):
        if (boundary.get("g") != generation or boundary.get("v") != schema or boundary.get("t") != tag
                or not _wall(boundary.get("_wall")) or boundary["_wall"] < hello["_wall"]):
            return unknown("known-technology scan belongs to an unverified generation")
    sequence = [_integer(record.get("_sequence")) for record in (hello, previous, anchor, day)]
    if (any(value is None for value in sequence) or not sequence[0] < sequence[1] < sequence[2] < sequence[3]
            or not previous["_wall"] <= anchor["_wall"] <= day["_wall"]
            or _integer(anchor.get("techs")) != count):
        return unknown("known-technology scan interval or closing count is unverified")
    rows = group.get("rows")
    if not isinstance(rows, list) or not rows:
        return unknown("known-technology scan is missing")
    known = set()
    for row in rows:
        if not isinstance(row, dict):
            return unknown("known-technology scan contains a malformed row")
        index = _integer(row.get("tid"))
        # Weekly scans run near the start of the game day, before its daily
        # closing record. Wall timestamps have only one-second resolution;
        # trusted parser stream order proves which closed interval owns a row.
        since_previous = days_between(previous.get("_date"), row.get("_date"))
        until_close = days_between(row.get("_date"), anchor.get("_date"))
        row_sequence = _integer(row.get("_sequence"))
        if (row.get("t") != tag or row.get("n") != scan_day or index is None or index >= len(catalog)
                or not _wall(row.get("_wall")) or not previous["_wall"] <= row["_wall"] <= anchor["_wall"]
                or row_sequence is None or not sequence[1] < row_sequence < sequence[2]
                or since_previous is None or not 0 <= since_previous <= 1
                or until_close is None or not 0 <= until_close <= 1):
            return unknown("known-technology scan contains foreign, stale or unresolved rows")
        tid = catalog[index]
        if tid in known:
            return unknown("known-technology scan contains duplicate rows")
        known.add(tid)
    if len(known) != count:
        return unknown("known-technology scan does not match the current technology count")
    return sorted(known), {"known_as_of_day": scan_day}
