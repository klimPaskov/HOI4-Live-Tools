"""Evaluate triggers in the running game, through the console, and say which condition is unmet.

A trigger cannot be run on its own from the console, an effect can. So each condition is wrapped in an effect that
only logs the answer:

    eval_effect GER = { if = { limit = { has_war = yes } log = "HLTa1b_3 TRUE" else = { log = "HLTa1b_3 FALSE" } } }

The `log` effect writes to game.log (`[time][date][effectbase.cpp:1783]: HLTa1b_3 TRUE`), where the answer is read
back by its tag. The console takes about 255 characters per command, so a condition longer than MAX_COMMAND minus
the wrapper cannot be asked as one piece: it is split into its children when it is a logic block and reported as
unknown otherwise. Nothing here changes the game: the wrapper has no effect other than the log line.

A condition is a Node. A false block is explained by evaluating its children (`depth` levels deep): the children
that make it fail are marked `blocking`.
"""

import re
import time
import uuid

from ..errors import ToolError
from ..gamedata import triggertext as tt
from . import console

MAX_COMMAND = 250
POLL_SECONDS = 6.0
CHUNK = 20                      # commands per console session
# the `log` effect's line ends with the text; a console echo of the command would carry more text after it
RESULT_RX = re.compile(r"\]:\s+(HLT[0-9a-f]{3})_(\d+)\s+(TRUE|FALSE)\s*$", re.M)


class Node:
    """One condition: what was asked (`stmt`), what is evaluated for it (`eval_stmt`, a child of a scope block is
    wrapped in its scope), the answer, and the explanation of a false block."""

    def __init__(self, stmt, kind=None, label=None, wrappers=()):
        self.stmt = stmt
        self.wrappers = tuple(wrappers)                 # the scope blocks it sits in, outermost first
        self.eval_stmt = _wrap(stmt, self.wrappers)
        self.kind = kind
        self.label = label
        self.result = None
        self.note = None
        self.children = []
        self.explain = True        # a false block is explained by its children (off for completed / bypass)

    def text(self):
        return self.stmt.render()

    def as_dict(self):
        d = {"condition": self.label or self.text()}
        if self.kind:
            d["kind"] = self.kind
        d["result"] = self.result
        if self.note:
            d["note"] = self.note
        if self.children:
            d["because"] = [c.as_dict() for c in self.children]
        return d


def _wrap(stmt, wrappers):
    for key, op in reversed(wrappers):
        stmt = tt.Stmt(key, op, children=[stmt])
    return stmt


SCOPE_KEY = re.compile(r"^([A-Z][A-Z0-9]{2}|[0-9]{1,5}|(ROOT|FROM|PREV|THIS)(\.\w+)*|all_\w+|owner|controller|overlord|"
                       r"faction_leader|capital_scope)$")


def drillable(key):
    """Blocks whose children can be asked one by one: AND / OR / NOT, and scope blocks (a tag, a state id, ROOT,
    owner, all_* ...), where every child must hold. Parameter blocks (check_variable, has_idea_with_trait ...) and
    any_* (all children must hold for the same element) are not split: a part of them means nothing."""
    return bool(key) and (key.upper() in tt.LOGIC_KEYS or bool(SCOPE_KEY.match(key)))


def command(scope, trigger_text, run, n):
    inner = (f'if = {{ limit = {{ {trigger_text} }} log = "{run}_{n} TRUE" else = {{ log = "{run}_{n} FALSE" }} }}')
    return f"eval_effect {scope} = {{ {inner} }}" if scope else f"eval_effect {inner}"


def parse_results(text, run):
    """{n: bool} for the answers of this run in a stretch of game.log."""
    out = {}
    for m in RESULT_RX.finditer(text):
        if m.group(1) == run:
            out[int(m.group(2))] = m.group(3) == "TRUE"
    return out


def evaluate(session, scope, nodes, poll=None):
    """Fill node.result (True, False or None when unknown) for every node; returns the error.log lines it caused."""
    run = "HLT" + uuid.uuid4().hex[:3]
    asked = {}
    cmds = []
    for node in nodes:
        n = len(cmds) + 1
        cmd = command(scope, node.eval_stmt.render(), run, n)
        if len(cmd) > MAX_COMMAND:
            node.note = f"too long for the console ({len(cmd)} of {MAX_COMMAND} characters)"
            continue
        asked[n] = node
        cmds.append(cmd)
    if not cmds:
        return []
    session.logs.mark("trigger_check", ("game", "error"))
    for i in range(0, len(cmds), CHUNK):
        session.check_cancel()
        console.run(session, cmds[i:i + CHUNK], read=False)
    deadline = time.time() + (POLL_SECONDS if poll is None else poll)
    got = {}
    while True:
        text, _ = session.logs.read_since("game", "trigger_check")
        got = parse_results(text, run)
        if len(got) >= len(asked) or time.time() >= deadline:
            break
        session.sleep(0.5)
    for n, node in asked.items():
        node.result = got.get(n)
        if node.result is None:
            node.note = "no answer in game.log (a trigger the game does not know, or the wrong scope; see errors)"
    rows, _ = session.logs.delta("error", "trigger_check", limit=20)
    return [r["line"] for r in rows]


def combine(key, results):
    """The result of a logic block from its children's results (None when it cannot be told)."""
    key = (key or "").upper()
    if key == "OR":
        return True if any(r is True for r in results) else (False if all(r is False for r in results) else None)
    if key == "NOT":
        return False if any(r is True for r in results) else (True if all(r is False for r in results) else None)
    if key in ("AND", ""):
        return False if any(r is False for r in results) else (True if all(r is True for r in results) else None)
    return None


def _children_of(node):
    """Child nodes of a block, each evaluated on its own. Logic blocks evaluate the child as it is; a scope block
    (a tag, ROOT, owner, all_* ...) evaluates it inside the same scope, and the scopes of the parents stay around
    it, so a grandchild is asked in the right country."""
    stmt = node.stmt
    if not stmt.is_block or not stmt.children or not drillable(stmt.key):
        return []
    logic = stmt.key.upper() in tt.LOGIC_KEYS
    wrappers = node.wrappers if logic else node.wrappers + ((stmt.key, stmt.op),)
    return [Node(c, wrappers=wrappers) for c in stmt.children]


def _blocking(node):
    """Mark the children that make a false block false."""
    key = (node.stmt.key or "").upper()
    for c in node.children:
        if key == "OR":
            c.note = c.note or ("false: every child of an OR must be false for it to fail" if c.result is False else None)
        elif key == "NOT":
            if c.result is True:
                c.note = "true, but a NOT needs it false"
        elif c.result is False:
            c.note = c.note or "false"


def check(session, scope, nodes, depth=2, max_checks=60):
    """Evaluate the nodes, then explain each false block by its children (depth levels). Returns
    {"result", "checked", "errors", "truncated"}; result is True, False or None."""
    errors = evaluate(session, scope, nodes)
    checked = len([n for n in nodes if n.result is not None or n.note])
    truncated = False
    for n in nodes:
        if n.explain and n.result is False and n.stmt.is_block and n.stmt.children and not drillable(n.stmt.key):
            n.note = n.note or f"false ({n.stmt.key} blocks are not split into smaller conditions)"
    level = [n for n in nodes if n.explain and n.result is False and n.stmt.is_block and n.stmt.children
             and drillable(n.stmt.key)]
    # a block that was too long to ask whole is answered from its children
    for n in [n for n in nodes if n.result is None and n.stmt.is_block and n.stmt.children
              and "too long" in (n.note or "")]:
        level.append(n)
    for _ in range(max(0, depth - 1)):
        batch = []
        for n in level:
            n.children = _children_of(n)
            batch.extend(n.children)
        if not batch:
            break
        if checked + len(batch) > max_checks:
            truncated = True
            for n in level:
                n.children = []
            break
        errors += evaluate(session, scope, batch)
        checked += len(batch)
        for n in level:
            if n.result is None and "too long" in (n.note or ""):
                n.result = combine(n.stmt.key, [c.result for c in n.children])
                if n.result is not None:
                    n.note = None
            _blocking(n)
        level = [c for n in level for c in n.children if c.stmt.is_block and c.stmt.children
                 and (c.result is False or (c.result is None and "too long" in (c.note or "")))]
    return {"result": overall(nodes), "checked": checked, "errors": errors[:20], "truncated": truncated}


def overall(nodes):
    """All of the nodes at once (a trigger block is an AND): False if one is false, True if all are, else None."""
    results = [n.result for n in nodes]
    return False if any(r is False for r in results) else (True if all(r is True for r in results) else None)


def unmet(nodes):
    return [n.as_dict() for n in nodes if n.result is False]


def unknown(nodes):
    return [n.as_dict() for n in nodes if n.result is None]


def require_trigger(text):
    """The conditions of a pasted trigger, or a ToolError that says what is wrong with it."""
    try:
        stmts = tt.conditions(text)
    except tt.ScriptError as exc:
        raise ToolError(f"the trigger does not parse: {exc}", {"error": "trigger_syntax"})
    if not stmts:
        raise ToolError("the trigger is empty", {"error": "trigger_syntax"})
    return stmts
