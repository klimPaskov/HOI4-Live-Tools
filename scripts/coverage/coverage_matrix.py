#!/usr/bin/env python3
"""The whole-game coverage matrix: what the game offers a player, against what the project can do.

    python3 coverage_matrix.py --game GAME_DIR [--mod MOD_DIR ...] [--repo REPO] [--runs DIR] [--plan PLAN.json]
                               [--commands FACTORIES.txt] [--rows rows.py] [--out-json OUT.json] [--out-md OUT.md]
                               [--baseline OLD.json] [--strict] [--write-baseline OUT.json]
    python3 coverage_matrix.py --inventory scripts/coverage/baseline.json --repo . --baseline scripts/coverage/baseline.json --strict

CI (no game files): `--inventory` takes the inventory of a committed baseline instead of reading a game, and
`--baseline` names which checks are strict (they passed when the baseline was written) and which only report.
`--write-baseline` writes that file from a run with `--game` (after a game update or a fix that makes a check pass).

Two parts.
1. The INVENTORY, computed from the game (and the mods layered over it, a mod file replacing the game file of the
   same relative path): every folder of common/ with its entity keys (special parsers for the folders whose keys
   sit one level down), the air and naval mission tables of the defines, every interface .gui file with its windows
   and its actionable elements (buttons, checkboxes, edit boxes) and their shortcuts, the keybinds, and from the
   executable (if found under GAME_DIR): the alert table (alert_* names), the command classes and the diplomatic
   action classes (Itanium-mangled RTTI names, <length><name>). Nothing here is hand-written, so a new game version
   or a total-conversion mod shows its new folders, windows, alerts and commands at once.
2. The JOINS: the MCP tool registry of REPO (Rust json! "name" entries, `const ...: &str = "hoi4_..."`, the macOS
   `supports` list and fleet/faction TOOLS arrays, Python @tool), the executor (plan_*.rs, test modules cut off),
   the monitor (hoi4_live/observe and plan_omissions.rs), the strategist (skills/, scripts/strategist), the plan,
   and the run receipts (RUNS/*/COVERAGE.md tables). The curated rows (rows.py: one row per player decision, with
   search patterns and the impact judgement) are evaluated against these joins; a row's state is computed from the
   evidence unless the row states it with a reason.

Checks (the "permanent checks"; exit code 2 with --strict when any fails): every alert has a lane, a row or a stated
reason; every command class is classified; every registered tool is exercised in a run or listed; every common/
folder is known to the baseline (new folders = new mechanics).
Stdlib only. Never writes into GAME_DIR or a mod.
"""
import argparse
import collections
import glob
import importlib.util
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import pdx  # noqa: E402

# ---------------------------------------------------------------- layered file access


class Layers:
    def __init__(self, game, mods):
        self.roots = [game] + list(mods or [])

    def files(self, rel, pattern='**/*.txt'):
        """Relative paths under rel, a later root replacing the same relative path."""
        out = {}
        for root in self.roots:
            base = os.path.join(root, rel)
            for f in glob.glob(os.path.join(base, pattern), recursive=True):
                if os.path.isfile(f):
                    out[os.path.relpath(f, root)] = f
        return dict(sorted(out.items()))

    def dirs(self, rel):
        out = set()
        for root in self.roots:
            base = os.path.join(root, rel)
            if os.path.isdir(base):
                out |= {d for d in os.listdir(base) if os.path.isdir(os.path.join(base, d))}
        return sorted(out)


# entity keys that sit one level below a wrapper key (folder -> wrapper keys)
WRAPPED = {
    'units': ('sub_units',), 'units/equipment': ('equipments',), 'units/equipment/modules': ('equipment_modules',),
    'buildings': ('buildings',), 'resources': ('resources',), 'abilities': ('ability',), 'raids': ('types',),
    'raids/categories': ('categories',), 'unit_leader': ('leader_traits',), 'medals': ('medals',),
    'unit_medals': ('unit_medals',), 'ribbons': ('ribbons',), 'aces': ('modifiers',), 'terrain': ('categories',),
    'wargoals': ('wargoal_types',), 'technology_tags': ('technology_folders',), 'opinion_modifiers': ('opinion_modifiers',),
    'state_category': ('state_categories',), 'autonomous_states': ('autonomy_state',), 'difficulty_settings': ('difficulty_settings',),
    'intelligence_agencies': ('intelligence_agency',), 'peace_conference': ('peace_action_categories',),
    'technologies': ('technologies',), 'decisions': (),
}


def entity_keys(layers, folder, wrappers=None):
    keys = []
    for rel, path in layers.files('common/' + folder).items():
        sub = os.path.relpath(os.path.dirname(rel), 'common/' + folder)
        if folder == 'units' and sub not in ('.', ''):
            continue  # units/equipment and units/codenames are their own entries
        tree = pdx.read(path)
        if wrappers:
            for k, v in tree:
                if k in wrappers and isinstance(v, list):
                    keys += [kk for kk, vv in v if kk and not kk.startswith('@') and isinstance(vv, list)]
        elif folder == 'ideas':
            for k, v in tree:
                if k == 'ideas' and isinstance(v, list):
                    keys += [kk for kk, vv in v if kk]
        else:
            keys += [k for k, v in tree if k and not k.startswith('@')]
    return keys


def idea_law_categories(layers):
    out = collections.OrderedDict()
    for rel, path in layers.files('common/ideas').items():
        for k, v in pdx.read(path):
            if k != 'ideas' or not isinstance(v, list):
                continue
            for cat, body in v:
                if isinstance(body, list) and pdx.first(body, 'law') == 'yes':
                    out.setdefault(cat, 0)
                    out[cat] += sum(1 for kk, vv in body if kk and isinstance(vv, list))
    return dict(out)


def defines_table(layers, block, table):
    """Names from the comments of a defines table, e.g. NAir MISSION_COMMAND_POWER_COSTS (one '-- NAME' per value)."""
    names = []
    for rel, path in layers.files('common/defines', '*.lua').items():
        text = open(path, encoding='utf-8', errors='replace').read()
        m = re.search(r'\b%s\s*=\s*\{' % block, text)
        if not m:
            continue
        t = re.search(r'\b%s\s*=\s*\{(.*?)\n\s*\}' % table, text[m.end():], re.S)
        if t:
            names = [re.sub(r'\s+', '_', c.strip())
                     for c in re.findall(r'--\s*([A-Z_][A-Z_ ]*[A-Z_])', t.group(1))]
    return names


def gui_inventory(layers):
    out = []
    act_types = ('buttonType', 'checkboxType', 'editBoxType', 'guiButtonType')
    for rel, path in layers.files('interface', '**/*.gui').items():
        tree = pdx.read(path)
        windows, elems, shortcuts, names = [], 0, 0, []

        def walk(block, depth):
            nonlocal elems, shortcuts
            for k, v in block:
                if not isinstance(v, list):
                    continue
                if k in act_types:
                    elems += 1
                    n = pdx.first(v, 'name')
                    if isinstance(n, str):
                        names.append(n)
                    if pdx.first(v, 'shortcut'):
                        shortcuts += 1
                if k == 'containerWindowType' and depth <= 1:
                    n = pdx.first(v, 'name')
                    if isinstance(n, str):
                        windows.append(n)
                walk(v, depth + 1)
        for k, v in tree:
            if k == 'guiTypes' and isinstance(v, list):
                walk(v, 0)
        out.append({'file': rel.split('interface/', 1)[-1], 'windows': windows, 'actionable': elems,
                    'shortcuts': shortcuts, 'element_names': names})
    return out


def find_binary(game):
    for c in ('hoi4.app/Contents/MacOS/hoi4', 'hoi4', 'hoi4.exe'):
        p = os.path.join(game, c)
        if os.path.isfile(p):
            return p
    return None


ALERT_NOT = {'alert_counter', 'alert_counter_value', 'alert_days', 'alert_level', 'alert_margin', 'alert_stack',
             'alert_text', 'alert_manager_update'}


def binary_inventory(path):
    if not path:
        return {'alerts': [], 'commands': [], 'actions': [], 'binary': None}
    b = open(path, 'rb').read()
    raw = set(m.decode() for m in re.findall(rb'(?<![a-z_])alert_[a-z_]{3,}(?![a-z_])', b))
    raw -= ALERT_NOT | {'alert'}
    # a name that extends another alert name is that alert's tooltip variant (_delayed, _instant, _entry ...)
    alerts = sorted(a for a in raw if not any(a.startswith(b + '_') for b in raw if b != a))
    cmds, acts = set(), set()
    for m in re.finditer(rb'(\d{1,3})(C[A-Z][A-Za-z0-9_]+)', b):
        n = int(m.group(1))
        s = m.group(2)[:n].decode()
        if len(s) != n or s.startswith('CCommandHelper'):
            continue
        if s.endswith('Command'):
            cmds.add(s)
        elif s.endswith('Action'):
            acts.add(s)
    return {'alerts': alerts, 'commands': sorted(cmds), 'actions': sorted(acts), 'binary': os.path.basename(path)}


def inventory(game, mods, commands_file=None):
    L = Layers(game, mods)
    folders = {}
    for d in L.dirs('common'):
        files = L.files('common/' + d, '**/*')
        txt = [f for f in files if f.endswith('.txt')]
        keys = entity_keys(L, d, WRAPPED.get(d) or None) if txt else []
        folders[d] = {'files': len(files), 'keys': len(keys), 'sample': keys[:8]}
    ent = {}
    for name, folder in [('raids', 'raids'), ('raid_categories', 'raids/categories'), ('operations', 'operations'),
                         ('special_projects', 'special_projects/projects'),
                         ('project_specializations', 'special_projects/specialization'),
                         ('occupation_laws', 'occupation_laws'), ('resistance_activities', 'resistance_activity'),
                         ('agency_upgrades', 'intelligence_agency_upgrades'), ('abilities', 'abilities'),
                         ('grand_doctrines', 'doctrines/grand_doctrines'), ('subdoctrines', 'doctrines/subdoctrines'),
                         ('doctrine_tracks', 'doctrines/tracks'), ('doctrine_folders', 'doctrines/folders'),
                         ('sub_units', 'units'), ('equipment', 'units/equipment'),
                         ('modules', 'units/equipment/modules'), ('buildings', 'buildings'), ('resources', 'resources'),
                         ('tech_folders', 'technology_tags'), ('game_rules', 'game_rules'),
                         ('timed_activities', 'timed_activities'), ('scientist_traits', 'scientist_traits'),
                         ('operation_tokens', 'operation_tokens'), ('operation_phases', 'operation_phases'),
                         ('faction_goals', 'factions/goals'), ('faction_rules', 'factions/rules'),
                         ('faction_upgrades', 'factions/upgrades'), ('faction_member_upgrades', 'factions/member_upgrades'),
                         ('faction_templates', 'factions/templates'), ('decision_categories', 'decisions/categories'),
                         ('medals', 'medals'), ('unit_medals', 'unit_medals'), ('ace_modifiers', 'aces'),
                         ('terrain_categories', 'terrain'), ('wargoals', 'wargoals'), ('bop', 'bop'),
                         ('mio_organisations', 'military_industrial_organization/organizations'),
                         ('dynamic_modifiers', 'dynamic_modifiers'), ('technology_sharing', 'technology_sharing'),
                         ('scripted_diplomatic_actions', 'scripted_diplomatic_actions'),
                         ('peace_conference', 'peace_conference')]:
        ent[name] = entity_keys(L, folder, WRAPPED.get(folder) or None)
    # leader traits by type
    lt = collections.Counter()
    for rel, path in L.files('common/unit_leader').items():
        for k, v in pdx.read(path):
            if k == 'leader_traits' and isinstance(v, list):
                for kk, vv in v:
                    if kk and isinstance(vv, list):
                        t = pdx.first(vv, 'type', 'unspecified')
                        lt[t if isinstance(t, str) else 'several'] += 1
    ent['law_categories'] = idea_law_categories(L)
    ent['leader_trait_types'] = dict(lt)
    ent['air_missions'] = defines_table(L, 'NAir', 'MISSION_COMMAND_POWER_COSTS')
    ent['naval_missions'] = defines_table(L, 'NNavy', 'MISSION_FUEL_COSTS')
    kb = []
    for rel, path in L.files('keybinds').items():
        kb += [k for k, v in pdx.read(path) if k]
    ent['keybinds'] = kb
    gui = gui_inventory(L)
    binv = binary_inventory(find_binary(game))
    if commands_file and os.path.isfile(commands_file):
        extra = set()
        for line in open(commands_file):
            parts = line.split()
            if len(parts) >= 2:
                x = parts[1].rstrip(':')
                inner = re.findall(r'\d+(C[A-Za-z0-9]+?)E?$', x) if x.startswith('N') else [x]
                if inner:
                    extra.add(inner[-1])
        binv['commands'] = sorted(set(binv['commands']) | extra)
        binv['commands_file'] = os.path.basename(commands_file)
    return {'game': os.path.abspath(game), 'mods': [os.path.abspath(m) for m in mods or []], 'folders': folders,
            'entities': ent, 'gui': gui, **binv}


# ---------------------------------------------------------------- repo joins


def read_text(path, cut_tests=False):
    try:
        t = open(path, encoding='utf-8', errors='replace').read()
    except OSError:
        return ''
    if cut_tests:
        m = re.search(r'#\[cfg\(test\)\]\s*(pub(\(\w+\))?\s+)?mod \w+\s*\{', t)
        if m:
            t = t[:m.start()]
    return t


def strip_comments(t):
    """Rust/C comments out (doc comments describe intent; the evidence must be code). Line numbers kept."""
    t = re.sub(r'/\*.*?\*/', lambda m: '\n' * m.group(0).count('\n'), t, flags=re.S)
    return re.sub(r'(?m)^(\s*)//.*$|(?<=[;{}),\s])//[^\n"]*$', lambda m: m.group(1) or '', t)


def corpus(paths, cut_tests=False):
    out = []
    for p in paths:
        for f in sorted(glob.glob(p, recursive=True)):
            if os.path.isfile(f) and '/tests/' not in f and not f.endswith('_tests.rs') and not os.path.basename(f).startswith('test_'):
                out.append((f, read_text(f, cut_tests)))
    return out


def registry(repo):
    src = corpus([repo + '/native/src/**/*.rs'], cut_tests=True)
    names = set()
    for f, t in src:
        names |= set(re.findall(r'"name"\s*:\s*"(hoi4_[a-z0-9_]+)"', t))
        names |= set(re.findall(r'const [A-Z_]+: &str = "(hoi4_[a-z0-9_]+)"', t))
        for arr in re.findall(r'const (?:TOOLS|READS)[^=]*=\s*\[(.*?)\];', t, re.S):
            names |= set(re.findall(r'"(hoi4_[a-z0-9_]+)"', arr))
        m = re.search(r'fn supports\(&self, name: &str\) -> bool \{(.*?)\n    \}', t, re.S)
        if m:
            names |= set(re.findall(r'"(hoi4_[a-z0-9_]+)"', m.group(1)))
    for f, t in corpus([repo + '/hoi4_live/tools/*.py']):
        names |= set(re.findall(r'@tool\(\s*"(hoi4_[a-z0-9_]+)"', t))
    return sorted(n for n in names if not n.endswith('_'))


def run_coverage(runs_dir):
    agg = collections.defaultdict(lambda: {'runs': 0, 'called_runs': 0, 'ok': 0, 'verified': 0, 'runs_ok': []})
    if not runs_dir:
        return {}

    def num(s):
        m = re.search(r'\d+', s or '')
        return int(m.group()) if m else 0
    for f in sorted(glob.glob(os.path.join(runs_dir, '*', 'COVERAGE.md'))):
        run = os.path.basename(os.path.dirname(f))
        hdr = None
        for line in open(f, encoding='utf-8', errors='replace'):
            if line.startswith('| tool'):
                hdr = [c.strip() for c in line.strip().strip('|').split('|')]
                continue
            if not line.startswith('| hoi4_') or not hdr:
                continue
            row = dict(zip(hdr, [c.strip() for c in line.strip().strip('|').split('|')]))
            a = agg[row['tool']]
            a['runs'] += 1
            if 'called by' in row:
                if 'NOT CALLED' in row['called by']:
                    continue
                ok, ver = num(row.get('ok')), num(row.get('verified'))
            else:
                if num(row.get('called')) == 0:
                    continue
                ok, ver = num(row.get('succeeded')), num(row.get('verified'))
            a['called_runs'] += 1
            a['ok'] += ok
            a['verified'] += ver
            if ok:
                a['runs_ok'].append(run)
    return dict(agg)


def run_receipts(runs_dir):
    """Verified action receipts of every run (RUNS/*/game-actions.jsonl): {kind: [(action dict, run)]}."""
    out = collections.defaultdict(list)
    if not runs_dir:
        return out
    for f in sorted(glob.glob(os.path.join(runs_dir, '*', 'game-actions.jsonl'))):
        run = os.path.basename(os.path.dirname(f))
        for line in open(f, encoding='utf-8', errors='replace'):
            if '"Verified"' not in line:
                continue
            try:
                d = json.loads(line)
            except ValueError:
                continue
            a = d.get('action') or {}
            if d.get('status') == 'Verified' and a.get('kind'):
                out[a['kind']].append((a, run))
    return out


class Joins:
    def __init__(self, repo, runs, plan):
        self.repo = repo
        self.tools = registry(repo)
        self.cov = run_coverage(runs)
        self.receipts = run_receipts(runs)
        self.exec = [(f, strip_comments(t)) for f, t in corpus([repo + '/native/src/plan_*.rs', repo + '/native/src/plan_*/**/*.rs'], cut_tests=True)]
        self.toolsrc = corpus([repo + '/native/src/launch/macos.rs', repo + '/native/src/launch/macos/**/*.rs',
                               repo + '/native/src/launch/linux*.rs', repo + '/hoi4_live/tools/*.py',
                               repo + '/native/src/construction/**/*.rs', repo + '/native/src/protocol.rs'], cut_tests=True)
        self.mon = corpus([repo + '/hoi4_live/observe/*.py', repo + '/native/src/plan_omissions.rs',
                           repo + '/native/src/plan_issues.rs'], cut_tests=True)
        self.strat = corpus([repo + '/skills/**/*.md', repo + '/skills/**/*.json', repo + '/scripts/strategist/**/*.py'])
        self.inproc = corpus([repo + '/inproc/**/*.c', repo + '/inproc/**/*.h', repo + '/inproc/**/*.py'])
        self.plan = read_text(plan) if plan else ''
        self.exec_tools = set()
        for f, t in self.exec:
            self.exec_tools |= set(re.findall(r'"(hoi4_[a-z0-9_]+)"', t))

    def hits(self, which, pattern, limit=2):
        if not pattern:
            return []
        rx = re.compile(pattern, re.I)
        out = []
        for f, t in getattr(self, which):
            for m in rx.finditer(t):
                line = t.count('\n', 0, m.start()) + 1
                out.append('%s:%d' % (os.path.relpath(f, self.repo), line))
                break
            if len(out) >= limit:
                break
        return out

    def plan_hits(self, pattern):
        if not pattern or not self.plan:
            return 0
        return len(re.findall(pattern, self.plan, re.I))

    def proof(self, tools, spec=None):
        """Live proof: a Verified receipt of the row's own action (spec {kind: {field: regex}}), else, for a tool
        that never writes receipts (a read), a successful call in a run's COVERAGE table."""
        if spec:
            for kind, fields in spec.items():
                hits = [(a, r) for a, r in self.receipts.get(kind, [])
                        if all(re.search(rx, str(a.get(k, '')), re.I) for k, rx in fields.items())]
                if hits:
                    return ('hoi4_' + kind + ' ' + ','.join('%s~%s' % kv for kv in fields.items()), len(hits), len(hits),
                            len({r for _, r in hits}))
            return None
        best = None
        for t in tools:
            kind = t[5:]
            if kind in self.receipts:
                hits = self.receipts[kind]
                cand = (t + ' (receipts)', len(hits), len(hits), len({r for _, r in hits}))
            else:
                c = self.cov.get(t)
                if not (c and c['ok']):
                    continue
                cand = (t + ' (read)', c['ok'], c['verified'], len(c['runs_ok']))
            if not best or cand[1] > best[1]:
                best = cand
        return best


def evaluate(row, J):
    tools = [t for t in row.get('tools', []) if t in J.tools]
    missing = [t for t in row.get('tools', []) if t not in J.tools]
    ex = J.hits('exec', row.get('ex'))
    # a read tool the executor calls proves no rule for this row; an action tool it calls does
    used = sorted(t for t in set(tools) & J.exec_tools if t[5:] in J.receipts)
    mon = J.hits('mon', row.get('mon'))
    st = J.hits('strat', row.get('st'))
    ph = J.plan_hits(row.get('plan'))
    pr = J.proof(tools, row.get('proof'))
    rule = bool(ex or used)
    if tools and rule and pr:
        state = 'proven'
    elif tools and rule:
        state = 'unproven'
    elif tools:
        state = 'tool-only'
    elif rule or mon:
        state = 'rule-without-tool'
    else:
        state = 'NOT HANDLED'
    computed = state
    if row.get('state'):
        state = row['state']
    return {
        'id': row['id'], 'domain': row['domain'], 'mechanic': row['m'], 'defined_in': row.get('src', ''),
        'tools': tools, 'tools_missing_from_registry': missing,
        'tool_proof': ({'tool': pr[0], 'ok': pr[1], 'verified': pr[2], 'runs': pr[3]} if pr else None),
        'executor': {'hits': ex, 'tools_called': used}, 'monitor': mon, 'strategist': st, 'plan_mentions': ph,
        'impact': row.get('imp', ''), 'impact_why': row.get('why', ''), 'state': state, 'state_computed': computed,
        'state_reason': row.get('reason', ''), 'work': row.get('work', ''),
        'decided_today': how_today(row, state, ph), 'general_decider': GEN.get(row['id'], ''),
    }


HOW, GEN = {}, {}
PLAN_ONLY = [lambda rid: False]


def how_today(row, state, plan_mentions):
    """How the decision is made today: fixed-plan / general-rule / runtime-computed / player-model / nobody."""
    if row['id'] in HOW:
        return {'how': HOW[row['id']][0], 'evidence': HOW[row['id']][1]}
    if state == 'n/a':
        return {'how': 'n/a', 'evidence': ''}
    if state in ('NOT HANDLED', 'rule-without-tool'):
        return {'how': 'nobody', 'evidence': 'monitor only' if state == 'rule-without-tool' else ''}
    if state == 'tool-only':
        return {'how': 'player-model', 'evidence': 'a tool, no rule'}
    if PLAN_ONLY[0](row['id']):
        if plan_mentions:
            return {'how': 'fixed-plan', 'evidence': 'the lane acts on the plan entry (%d mentions)' % plan_mentions}
        return {'how': 'nobody', 'evidence': 'the lane acts only on a plan entry; this plan has none and no rule chooses it'}
    if row.get('plan') and plan_mentions:
        return {'how': 'fixed-plan', 'evidence': 'the plan names it (%d mentions)' % plan_mentions}
    return {'how': 'general-rule', 'evidence': 'executor rule without a plan entry'}


# ---------------------------------------------------------------- checks


def checks(inv, J, rows_eval, baseline, alert_owned):
    out = []
    # 1. every alert the game defines has a lane, a row or a stated reason
    unowned = [a for a in inv['alerts'] if a[len('alert_'):] not in alert_owned]
    out.append({'id': 'alerts_owned', 'check': 'every game alert has a lane, a row or a stated reason', 'pass': not unowned,
                'count': len(inv['alerts']), 'failing': unowned})
    # 2. every registered tool is exercised in a run or listed as not exercised
    py = set()
    for f, t in corpus([J.repo + '/hoi4_live/tools/*.py']):
        py |= set(re.findall(r'@tool\(\s*"(hoi4_[a-z0-9_]+)"', t))
    nat = [t for t in J.tools if t not in py]
    have_runs = bool(J.cov or J.receipts)
    never = [t for t in nat if not ((J.cov.get(t) or {}).get('ok') or t[5:] in J.receipts)]
    if have_runs:  # needs the recorded runs (--runs): the gate packet, not CI
        out.append({'id': 'tools_succeeded', 'check': 'every registered native tool succeeded in a recorded run (else listed here)',
                    'pass': not never, 'count': len(nat), 'failing': never})
    pyn = sorted(t for t in py if not (J.cov.get(t) or {}).get('ok'))
    if have_runs:
        out.append({'id': 'python_tools_succeeded', 'check': 'the Python server\'s tools (Windows/Linux legacy path) succeeded in a recorded run',
                    'pass': not pyn, 'count': len(py), 'failing': pyn})
    # 3. every command class is classified (named in the repo, mapped by a row, or a stated n/a family)
    blob = '\n'.join(t for _, t in J.toolsrc + J.exec + J.inproc)
    rowtext = json.dumps([r.get('commands', []) for r in ROWS])
    na = re.compile(r'Chat|Sync|Ping|HotJoin|Lobby|Player|Human|Chunk|FileTransfer|Checksum|Ready|RandomSeed|Achievements|'
                    r'Debug|Autosave|Quit|Profile|Badge|Seen|Name|Icon|Color|Logo|Insignia|Symbol|Sprite|Model|Highlight|'
                    r'Ai[A-Z]|Hourly|Bookmark|StartGame|Difficulty|GameRule|GamePlayOptions|DLCs|UniqueId|Controller|'
                    r'Expand|Scale|Reorder|Pin|Mark|Clear(Accident|Repair)|Unread|Interface|Selection|Dispatch|Spy|Codename')
    uncl = [c for c in inv['commands'] if c not in blob and c not in rowtext and not na.search(c)]
    # the curated classification (rows.py COMMAND_CLASSES): tool / n/a classify a class, gap keeps it failing
    named = {c: CMDCLS.get(c, '') for c in uncl}
    open_ = ['%s (%s)' % (c, w) if w else c for c, w in named.items() if not w.startswith(('tool:', 'n/a:', 'row:'))]
    out.append({'id': 'commands_classified', 'check': 'every command class has a tool, a row or an n/a family', 'pass': not open_,
                'count': len(inv['commands']), 'classified_by_name': len(uncl) - len(open_), 'failing': open_})
    # 4. every common/ folder is known to the baseline
    if baseline:
        new = sorted(set(inv['folders']) - set(baseline.get('folders', {})))
        newc = sorted(set(inv['commands']) - set(baseline.get('commands', [])))
        newa = sorted(set(inv['alerts']) - set(baseline.get('alerts', [])))
        out.append({'id': 'no_new_mechanics', 'check': 'no new common/ folder, command class or alert since the baseline',
                    'pass': not (new or newc or newa), 'failing': new + newc + newa})
    # 5. every row with impact high is handled
    hi = [r['id'] for r in rows_eval if r['impact'] == 'high' and r['state'] in ('NOT HANDLED', 'tool-only', 'rule-without-tool')]
    out.append({'id': 'high_impact_handled', 'check': 'no high-impact row is NOT HANDLED, tool-only or rule-without-tool', 'pass': not hi, 'failing': hi})
    nb = [r['id'] for r in rows_eval if r['impact'] == 'high' and r['decided_today']['how'] in ('nobody', 'player-model')]
    out.append({'id': 'high_impact_decided', 'check': 'every high-impact row is decided by a rule, an evaluator or the plan (not nobody / the player model)',
                'pass': not nb, 'failing': nb})
    fx = [r['id'] for r in rows_eval if r['impact'] == 'high' and r['decided_today']['how'] == 'fixed-plan' and not r['general_decider']]
    out.append({'id': 'fixed_plan_general_decider', 'check': 'every high-impact row fixed in a country plan names its general decider (for a country nobody researched)',
                'pass': not fx, 'failing': fx})
    return out


# ---------------------------------------------------------------- output

ROWS = []
CMDCLS = {}
STATES = ['proven', 'unproven', 'tool-only', 'rule-without-tool', 'NOT HANDLED', 'n/a']


def md_table(rows_eval):
    lines = []
    by = collections.OrderedDict()
    for r in rows_eval:
        by.setdefault(r['domain'], []).append(r)
    for dom, rs in by.items():
        lines.append('\n### %s (%d rows)\n' % (dom, len(rs)))
        lines.append('| id | mechanic (defined in) | tool (live proof) | executor | plan / strategist | monitor | impact | state | decided today | general decider (any country/mod) |')
        lines.append('|---|---|---|---|---|---|---|---|---|---|')
        for r in rs:
            tp = r['tool_proof']
            tool = ', '.join(r['tools']) or '-'
            if tp:
                tool += ' (ok %d, verified %d, %d runs)' % (tp['ok'], tp['verified'], tp['runs'])
            elif r['tools']:
                tool += ' (no live success)'
            ex = '; '.join(r['executor']['hits'][:1] + (['calls ' + ','.join(r['executor']['tools_called'][:3])]
                                                         if r['executor']['tools_called'] else [])) or '-'
            ps = 'plan %d; strat %s' % (r['plan_mentions'], 'yes' if r['strategist'] else 'no')
            mon = (r['monitor'][0] if r['monitor'] else '-')
            st = r['state'] + (' (' + r['state_reason'] + ')' if r['state_reason'] else '')
            if r['state'] != r['state_computed'] and not r['state_reason']:
                st += ' [computed %s]' % r['state_computed']
            imp = '%s: %s' % (r['impact'], r['impact_why']) if r['impact_why'] else r['impact']
            mech = '%s (%s)' % (r['mechanic'], r['defined_in']) if r['defined_in'] else r['mechanic']
            if r['work'] and r['state'] not in ('proven', 'n/a'):
                st += '. Work: ' + r['work']
            dt = r['decided_today']
            cells = [r['id'], mech, tool, ex, ps, mon, imp, st, dt['how'] + (' (' + dt['evidence'] + ')' if dt['evidence'] else ''),
                     r['general_decider'] or '-']
            lines.append('| ' + ' | '.join(c.replace('|', '/').replace('\n', ' ') for c in cells) + ' |')
    return '\n'.join(lines)


def counts_table(rows_eval):
    doms = list(collections.OrderedDict((r['domain'], 1) for r in rows_eval))
    c = collections.Counter((r['domain'], r['state']) for r in rows_eval)
    lines = ['| domain | rows | ' + ' | '.join(STATES) + ' |', '|---|---:|' + '---:|' * len(STATES)]
    for d in doms:
        n = sum(c[(d, s)] for s in STATES)
        lines.append('| %s | %d | ' % (d, n) + ' | '.join(str(c[(d, s)]) for s in STATES) + ' |')
    tot = collections.Counter(r['state'] for r in rows_eval)
    lines.append('| **all** | **%d** | ' % len(rows_eval) + ' | '.join('**%d**' % tot[s] for s in STATES) + ' |')
    return '\n'.join(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--game')
    ap.add_argument('--inventory', help='a JSON file holding a stored inventory (its "inventory" key), instead of --game')
    ap.add_argument('--mod', action='append', default=[])
    ap.add_argument('--repo')
    ap.add_argument('--runs')
    ap.add_argument('--plan')
    ap.add_argument('--commands')
    ap.add_argument('--rows', default=os.path.join(HERE, 'rows.py'))
    ap.add_argument('--baseline')
    ap.add_argument('--out-json')
    ap.add_argument('--out-md')
    ap.add_argument('--strict', action='store_true')
    ap.add_argument('--write-baseline', help='write the inventory and the checks that pass (strict) to this file')
    a = ap.parse_args()
    if a.inventory:
        with open(a.inventory) as f:
            stored = json.load(f)
        inv = stored.get('inventory', stored)
    elif a.game:
        inv = inventory(a.game, a.mod, a.commands)
    else:
        ap.error('--game or --inventory is required')
    result = {'inventory': inv}
    if a.repo:
        J = Joins(a.repo, a.runs, a.plan)
        global ROWS
        if os.path.isfile(a.rows):
            spec = importlib.util.spec_from_file_location('rows', a.rows)
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            ROWS = mod.rows(inv)
            alert_owned = mod.alert_owned(J)
            HOW.update(getattr(mod, 'HOW', {}))
            CMDCLS.update(getattr(mod, 'COMMAND_CLASSES', {}))
            GEN.update(getattr(mod, 'GEN', {}))
            if hasattr(mod, 'plan_only'):
                PLAN_ONLY[0] = mod.plan_only
        else:
            alert_owned = set()
        ev = [evaluate(r, J) for r in ROWS]
        base = None
        if a.baseline and os.path.isfile(a.baseline):
            with open(a.baseline) as f:
                base = json.load(f)
        result.update({'tools': J.tools, 'tool_coverage': J.cov, 'rows': ev,
                       'checks': checks(inv, J, ev, base and base.get('inventory'), alert_owned)})
        if a.out_md:
            with open(a.out_md, 'w') as f:
                hc = collections.Counter((r['domain'], r['decided_today']['how']) for r in ev)
                hows = ['fixed-plan', 'general-rule', 'runtime-computed', 'player-model', 'nobody', 'n/a']
                ht = ['| domain | ' + ' | '.join(hows) + ' |', '|---|' + '---:|' * len(hows)]
                for d in collections.OrderedDict((r['domain'], 1) for r in ev):
                    ht.append('| %s | ' % d + ' | '.join(str(hc[(d, h)]) for h in hows) + ' |')
                tot = collections.Counter(r['decided_today']['how'] for r in ev)
                ht.append('| **all** | ' + ' | '.join('**%d**' % tot[h] for h in hows) + ' |')
                f.write('## Counts\n\n' + counts_table(ev) + '\n\n### How each decision is made today\n\n' + '\n'.join(ht)
                        + '\n\n## The full matrix by domain\n' + md_table(ev) + '\n')
    if a.out_json:
        with open(a.out_json, 'w') as f:
            json.dump(result, f, indent=1, sort_keys=False)
    s = {k: (len(v) if isinstance(v, (list, dict)) else v) for k, v in inv['entities'].items()}
    print(json.dumps({'folders': len(inv['folders']), 'gui_files': len(inv['gui']),
                      'gui_actionable': sum(g['actionable'] for g in inv['gui']), 'alerts': len(inv['alerts']),
                      'commands': len(inv['commands']), 'actions': len(inv['actions']), 'entities': s}, indent=None))
    if 'checks' in result:
        for c in result['checks']:
            print(('PASS ' if c['pass'] else 'FAIL ') + c['check'] + ('' if c['pass'] else ' (%d)' % len(c['failing'])))
        if a.write_baseline:
            keep = dict(inv, game=os.path.basename(os.path.normpath(inv.get('game') or '')) or None,
                        mods=[os.path.basename(os.path.normpath(m)) for m in inv.get('mods') or []])
            with open(a.write_baseline, 'w') as f:
                json.dump({'strict': sorted(c['id'] for c in result['checks'] if c['pass']),
                           'report_only': {c['id']: len(c['failing']) for c in result['checks'] if not c['pass']},
                           'inventory': keep}, f, indent=1, sort_keys=True)
                f.write('\n')
        # strict: with a baseline, only the checks it lists as strict fail the run (the others report);
        # without one, every check
        strict = set((base or {}).get('strict') or []) if base and 'strict' in base else None
        bad = [c for c in result['checks'] if not c['pass'] and (strict is None or c['id'] in strict)]
        for c in bad:
            print('STRICT FAIL %s: %s' % (c['id'], ', '.join(map(str, c['failing'][:20]))))
        if a.strict and bad:
            sys.exit(2)


if __name__ == '__main__':
    main()
