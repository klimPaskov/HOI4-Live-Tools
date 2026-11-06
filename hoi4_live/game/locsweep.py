"""Localisation sweep: find untranslated and broken text on the game's screens, and localisation warnings in the logs.

The driver visits the bare map and then each requested panel, reads the whole screen with OCR and flags suspicious
lines. It also reads the `error` and `text` logs for localisation warnings written while it ran.

What counts as suspicious (checked in this order, a line gets the first kind that matches):

  missing_key           `[missing_key]` or `missing_key`, any case. The OCR confusions `i`/`l`/`1`/`|` for the two
                        letters `i` are tolerated. `[missing key]` with a space counts only inside brackets.
  unresolved_variable   a variable or loc command that was never replaced: `$NAME$`, `$NAME|Y$`, `[Root.GetName]`,
                        `[This.Owner.GetNameDef]`, `[GetDateText]`, `[?var_name|0]`, or a literal backslash n.
  colour_code           a section sign (U+00A7). The game never shows it, so any one is a colour code printed as text.
  raw_key               a localisation key printed instead of its text. Only a line that is exactly one identifier
                        counts: letters, digits, `_`, `.` and `-` with no spaces, 6 to 80 characters, starting with a
                        letter, at least 3 letters, and one of
                          - an underscore between parts: `GER_focus_name`, `FOCUS_DESC_x`, `my_mod.1.t`
                          - the event style with dots: three or more dot parts and one part that is only digits
                            (`news.1.t`)
                        Quotes around the line and trailing dots, commas, colons and exclamation marks are ignored
                        (OCR often adds them).
                        Not flagged: numbers, dates (`1936.1.1`), versions (`v1.19.4.1`), short tags and
                        abbreviations (`USA`, `OK`, `U.S.S.R`), file and web names (`config.json`, `x.paradoxplaza.com`)
                        and any line with a space, including prose that holds an underscore (`Press the Q_key`).
  custom                a match of a caller supplied regex (case-insensitive search), only for a line no built-in
                        rule flagged.

The rules favour precision over recall: a key printed in the middle of a longer OCR line is not found. OCR noise
is common, so repeated lines are folded per screen (case-insensitive, whitespace collapsed) and counted once.

`loc_log_lines` keeps the localisation lines of a log delta. `sweep` is the driver.
"""

import datetime
import re

from ..errors import ToolError
from . import panels as panels_mod

DEFAULT_PANELS = ("politics", "decisions", "research", "production", "construction", "trade", "deployment",
                  "logistics", "officer_corps", "faction", "army_overview", "navy_overview", "air_overview",
                  "intelligence")
KINDS = ("missing_key", "unresolved_variable", "colour_code", "raw_key", "custom")
LOGS = ("error", "text")
MARK = "loc_sweep"

_MISSING_RX = re.compile(r"m[i1l|]ss[i1l|]ng_key|\[\s*m[i1l|]ss[i1l|]ng[ _]key\s*\]", re.I)
_VARIABLE_RXS = (
    re.compile(r"\$[A-Za-z_][A-Za-z0-9_|.%+*-]*\$"),                                     # $NAME$ and $NAME|Y$
    re.compile(r"\[[A-Za-z_][A-Za-z0-9_.:]*\.Get[A-Z][A-Za-z0-9_]*(?:\([^)\]]*\))?(?:\|[^\]\s]*)?\]"),   # [Root.GetName]
    re.compile(r"\[Get[A-Z][A-Za-z0-9_]*(?:\([^)\]]*\))?(?:\|[^\]\s]*)?\]"),               # [GetDateText]
    re.compile(r"\[\?[A-Za-z_][^\]\s]*\]"),                                                # [?var_name|0]
    re.compile(r"\\n"),                                                                    # a literal backslash n
)
_COLOUR_RX = re.compile(chr(0xA7))
_KEY_SHAPE = re.compile(r"^[A-Za-z][A-Za-z0-9\-]*(?:[._][A-Za-z0-9\-]+)+$")
_VERSION_RX = re.compile(r"^[vV]?\d+(?:\.\d+)+$")
_KEY_MIN, _KEY_MAX = 6, 80


def compile_patterns(patterns):
    """Compile the caller's regexes (case-insensitive). Raises ToolError naming the bad one."""
    out = []
    for i, p in enumerate(patterns or ()):
        if hasattr(p, "search"):
            out.append(p)
            continue
        try:
            out.append(re.compile(p, re.I))
        except (re.error, TypeError) as e:
            raise ToolError(f"extra_patterns[{i}] is not a valid regex: {e}", {"field": f"extra_patterns[{i}]"})
    return out


def _is_raw_key(text):
    token = text.strip().strip("\"'`").rstrip(".,;:!")
    if not (_KEY_MIN <= len(token) <= _KEY_MAX) or not _KEY_SHAPE.match(token) or _VERSION_RX.match(token):
        return False
    if sum(c.isalpha() for c in token) < 3:
        return False
    if "_" in token:
        return True
    parts = token.split(".")                  # the event style: namespace.number.t
    return len(parts) >= 3 and any(p.isdigit() for p in parts)


def suspicious(text, extra_patterns=()):
    """The kind of problem a line of OCR text shows (see the module docstring), or None for a normal line.
    extra_patterns are regex strings or compiled patterns."""
    text = (text or "").strip()
    if not text:
        return None
    if _MISSING_RX.search(text):
        return "missing_key"
    if any(rx.search(text) for rx in _VARIABLE_RXS):
        return "unresolved_variable"
    if _COLOUR_RX.search(text):
        return "colour_code"
    if _is_raw_key(text):
        return "raw_key"
    for rx in compile_patterns(extra_patterns):
        if rx.search(text):
            return "custom"
    return None


def classify_lines(lines, extra_patterns=(), max_findings=40):
    """Flag the suspicious OCR lines of one screen. `lines` have .text and .box. Repeats are folded into one finding
    with a count. Returns {lines, total, by_kind, findings, omitted}: `total` and `by_kind` count every finding,
    `findings` lists at most max_findings of them in reading order and `omitted` counts the rest."""
    patterns = compile_patterns(extra_patterns)
    found = {}
    for line in lines:
        kind = suspicious(line.text, patterns)
        if not kind:
            continue
        key = " ".join(line.text.split()).casefold()
        if key in found:
            found[key]["count"] += 1
            continue
        found[key] = {"kind": kind, "text": " ".join(line.text.split()), "box": [int(v) for v in line.box], "count": 1}
    findings = sorted(found.values(), key=lambda f: (f["box"][1], f["box"][0]))
    by_kind = {}
    for f in findings:
        by_kind[f["kind"]] = by_kind.get(f["kind"], 0) + 1
    return {"lines": len(lines), "total": len(findings), "by_kind": by_kind,
            "findings": findings[:max_findings], "omitted": max(0, len(findings) - max_findings)}


# ---------------------------------------------------------------------------------------------------- logs
_LOC_LOG_RX = re.compile(r"locali[sz]ation|loc key|localization key|missing key|duplicate.*key|yml|l_english", re.I)


def loc_log_lines(rows):
    """The localisation lines of a log delta. `rows` are the dicts Logs.delta returns ({line, count?}). Returns
    [{line, count}] (count is 1 for a line seen once)."""
    out = []
    for row in rows or ():
        if _LOC_LOG_RX.search(row.get("line", "")):
            kept = {"line": row["line"], "count": row.get("count", 1)}
            if row.get("known_mistake"):
                kept["known_mistake"] = row["known_mistake"]
            out.append(kept)
    return out


def _read_logs(session, mark=None):
    """({log: localisation rows}, {log: reader info}) for the error and text logs since a mark (whole file for None)."""
    rows, info = {}, {}
    for log in LOGS:
        delta, meta = session.logs.delta(log, mark, limit=5000)
        rows[log] = loc_log_lines(delta)
        info[log] = {k: meta.get(k) for k in ("exists", "restarted", "truncated", "lines") if k in meta}
    return rows, info


# ---------------------------------------------------------------------------------------------------- driver
def _read_screen(session, name, extra_patterns, max_findings):
    img = session.grab()
    res = session.ocr.recognize(img, scale=1.5)
    row = {"screen": name, "status": "ok"}
    row.update(classify_lines(res.lines, extra_patterns, max_findings))
    row["screenshot"] = session.artifact(img, f"loc_sweep_{name}") if row["total"] else None
    return row


def sweep(session, panels=DEFAULT_PANELS, extra_patterns=(), since="start", max_findings=40):
    """Sweep the map and the requested panels, then read the logs. Returns the report dict:

      screens        one row per screen: screen, status (ok | not_opened | read_failed), lines (OCR lines read),
                     total, by_kind, findings, omitted, screenshot (a file, only when there are findings), message
      totals         screens, swept, not_opened, read_failed, with_findings, findings, by_kind
      log_warnings   {error: [{line, count}], text: [...]} written since the sweep started
      before_sweep   the same for what the logs held when the sweep started, only for since="all"
      log_info       what the log reader saw (exists, restarted, ...)
      returned_to_map, map_error

    A map that cannot be read stops the sweep with the error, a panel that cannot be opened or read is recorded and
    the sweep goes on. The bare map is restored at the end whatever happens."""
    patterns = compile_patterns(extra_patterns)
    session.logs.mark(MARK, LOGS)
    before = _read_logs(session)[0] if since == "all" else None
    names = ["map"] + list(panels)
    screens = []
    returned, map_error = False, None
    try:
        panels_mod.to_map(session)
        session.check_cancel()
        session.report("sweep: map", 1, len(names))
        screens.append(_read_screen(session, "map", patterns, max_findings))
        for i, name in enumerate(panels, start=2):
            session.check_cancel()
            session.report(f"sweep: {name}", i, len(names))
            screens.append(_sweep_panel(session, name, patterns, max_findings))
    finally:
        try:
            panels_mod.to_map(session)
            returned = True
        except Exception as e:                     # never hide the error that is already on its way out
            map_error = str(e)
    logs, log_info = _read_logs(session, MARK)
    report = {"started": datetime.datetime.now().isoformat(timespec="seconds"), "since": since, "screens": screens,
              "totals": _totals(screens), "log_warnings": logs, "log_info": log_info,
              "returned_to_map": returned, "map_error": map_error}
    if before is not None:
        report["before_sweep"] = before
    return report


def _sweep_panel(session, name, patterns, max_findings):
    try:
        panels_mod.open_panel(session, name)
    except ToolError as e:
        _reraise_cancel(e)
        _leave(session)
        return {"screen": name, "status": "not_opened", "message": str(e)}
    try:
        return _read_screen(session, name, patterns, max_findings)
    except ToolError as e:
        _reraise_cancel(e)
        return {"screen": name, "status": "read_failed", "message": str(e)}
    finally:
        _leave(session)


def _reraise_cancel(exc):
    """A client cancel is a ToolError too. It must stop the sweep, not be recorded as a failed panel."""
    from ..session import Cancelled
    if isinstance(exc, Cancelled):
        raise exc


def _leave(session):
    """Close what is open so the next panel starts from the map. A failure here is retried by the final to_map."""
    try:
        panels_mod.close(session)
    except ToolError as e:
        _reraise_cancel(e)


def _totals(screens):
    by_kind = {}
    for s in screens:
        for kind, n in s.get("by_kind", {}).items():
            by_kind[kind] = by_kind.get(kind, 0) + n
    return {"screens": len(screens),
            "swept": sum(1 for s in screens if s["status"] == "ok"),
            "not_opened": sum(1 for s in screens if s["status"] == "not_opened"),
            "read_failed": sum(1 for s in screens if s["status"] == "read_failed"),
            "with_findings": sum(1 for s in screens if s.get("total")),
            "findings": sum(s.get("total", 0) for s in screens),
            "by_kind": by_kind}


def warning_count(report):
    """Number of distinct localisation lines found in the logs since the sweep started."""
    return sum(len(v) for v in report["log_warnings"].values())


def summary(report):
    t = report["totals"]
    text = f"sweep: {t['findings']} finding(s) on {t['with_findings']} of {t['screens']} screen(s)"
    if t["not_opened"]:
        text += f"; {t['not_opened']} panel(s) not opened"
    if t["read_failed"]:
        text += f"; {t['read_failed']} panel(s) not read"
    text += f"; {warning_count(report)} localisation warning(s) in the logs"
    if not report["returned_to_map"]:
        text += "; could not return to the map"
    return text


def markdown(report):
    """The report as Markdown."""
    t = report["totals"]
    out = [f"# Localisation sweep\n\n{report['started']}, {summary(report)}\n"]
    out.append("| screen | status | OCR lines | findings | note |\n|---|---|---|---|---|")
    for s in report["screens"]:
        note = (s.get("message") or s.get("screenshot") or "").replace("|", "/")
        out.append(f"| {s['screen']} | {s['status']} | {s.get('lines', '')} | {s.get('total', '')} | {note} |")
    out.append("\nFindings by kind: " + (", ".join(f"{k} {n}" for k, n in sorted(t["by_kind"].items())) or "none") + "\n")
    for s in report["screens"]:
        if not s.get("findings"):
            continue
        out.append(f"## {s['screen']}\n")
        for f in s["findings"]:
            times = f" (x{f['count']})" if f["count"] > 1 else ""
            out.append(f"- {f['kind']}: `{f['text']}`{times}")
        if s.get("omitted"):
            out.append(f"- {s['omitted']} more not listed")
        out.append("")

    def log_block(title, rows_by_log):
        out.append(f"## {title}\n")
        n = 0
        for log, rows in rows_by_log.items():
            for r in rows:
                n += 1
                times = f" (x{r['count']})" if r["count"] > 1 else ""
                out.append(f"- {log}.log: `{r['line']}`{times}")
        if not n:
            out.append("None.")
        out.append("")

    log_block("Localisation warnings in the logs since the sweep started", report["log_warnings"])
    if "before_sweep" in report:
        log_block("Localisation warnings already in the logs when the sweep started", report["before_sweep"])
    if not report["returned_to_map"]:
        out.append(f"The sweep could not return to the map: {report['map_error']}\n")
    return "\n".join(out)
