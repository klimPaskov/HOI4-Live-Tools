"""The strategist skill (skills/hoi4-strategist) is complete and self-consistent.

(a) SKILL.md has frontmatter with name hoi4-strategist and a description;
(b) every repository path and relative link the skill's files name exists, except the files other
    build items create (EXPECTED_LATER);
(c) templates/run_spec.json parses and has the run-spec keys;
(d) the role prompts name no model provider (they are fed to any launcher);
(e) the playbook SKILL.md, its INDEX.md and AGENTS.md point to the skill;
(f) the 29-item campaign checklist is in procedure.md (one section per item with its facts command and
    resolution), in the plan.md coverage table, and enforced by SKILL.md step 5; the brief template has
    an exact call for every player-owned key; the rubric has 16 rows and the 22 of 32 pass mark.
"""

import json
import os
import re
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SKILL = os.path.join(ROOT, "skills", "hoi4-strategist")

# Named by the skill, created by other build items (design section 6), with the item that creates
# each. test_expected_later_are_referenced keeps this list from going stale.
EXPECTED_LATER = {
    "scripts/hoi4facts/dossier.py": "build item 6",
    "scripts/hoi4facts/facts.py": "build item 7",
    "scripts/hoi4facts/calc.py": "build item 7",
    "scripts/hoi4facts/lint_plan.py": "build item 8",
    "skills/hoi4-campaign-playbook/references/lessons.jsonl": "build item 9 (lessons builder)",
    "scripts/strategist/eval_plan.py": "build item 11",
    "scripts/strategist/reconcile.py": "build item 17",
    "scripts/strategist/packet.py": "build item 21",
    "scripts/strategist/revise.py": "build item 21",
}

REQUIRED_FILES = [
    "SKILL.md",
    "references/procedure.md",
    "references/goal-patterns.md",
    "references/rubric.md",
    "references/revision.md",
    "references/lessons.md",
    "templates/plan.md",
    "templates/brief.md",
    "templates/run_spec.json",
    "prompts/strategist.md",
    "prompts/critic.md",
    "prompts/revise.md",
    "prompts/fix.md",
    "prompts/answer.md",
    "agents/openai.yaml",
    "examples/README.md",
]

# The campaign checklist (coverage audit, section 3), in order. procedure.md has "### <n>. <name>" for each
# and templates/plan.md a coverage row "| <n> | <name> |".
CHECKLIST = [
    "Goal", "Situation", "Force posture", "Doctrine path", "XP budget", "Templates", "Equipment designs",
    "Production", "MIOs", "Economy timing", "Research", "Focus and balance of power", "Gated laws",
    "Stability and war support", "Advisors and high command", "Army organisation", "Manpower", "Operations",
    "Supply and logistics", "Air plan", "Naval plan", "Diplomacy", "Occupation", "Intelligence", "Peace aims",
    "Events and decisions", "Checkpoints and contingencies", "Continuous management", "FAIR boundary",
]

# Fact readers the procedure names before they exist (another build item adds them); each must come with a
# fallback ("until available").
PENDING_FACTS = ["terrain-width", "units", "weather", "occupation-laws", "bop", "ports"]

# Player-owned plan keys and the tool the brief template gives for each.
BRIEF_CALLS = {
    "templates": "hoi4_template_create", "politics.when": "hoi4_politics_choose",
    "decision_rules": "hoi4_decision_take", "army_group": "hoi4_army_group_create",
    "hold_targets": "hoi4_army_front", "retake_rules": "hoi4_army_move", "air_plan": "hoi4_air_mission",
    "sea_transfer": "hoi4_army_move",
    "navy": "hoi4_fleet_mission", "designs": "hoi4_design_create", "mio": "hoi4_mio_trait",
    "production replace": "hoi4_production_set", "auto_import": "hoi4_auto_import",
    "occupation": "hoi4_occupation_law", "diplomacy": "hoi4_diplomacy_act",
    "intelligence": "hoi4_agency_upgrade", "peace": "hoi4_peace_take",
}

REPO_PATH = re.compile(r"(?<![\w./-])((?:skills|scripts|tests|docs|native|hoi4_live)/[A-Za-z0-9_./-]+"
                       r"\.(?:md|py|jsonl|json|rs|yaml|txt))(?![A-Za-z0-9_])")
MD_LINK = re.compile(r"\]\(([^)#\s]+)(?:#[^)]*)?\)")
PROVIDERS = re.compile(r"\b(claude|anthropic|sonnet|haiku|opus|openai|codex|chatgpt|gpt-?\d*|gemini|"
                       r"llama|mistral)\b", re.I)


def skill_files():
    for base, _, names in os.walk(SKILL):
        for name in sorted(names):
            if name.endswith((".md", ".json", ".yaml")):
                yield os.path.join(base, name)


def read(path):
    with open(path, encoding="utf-8") as f:
        return f.read()


def frontmatter(text):
    m = re.match(r"^---\n(.*?)\n---\n", text, re.S)
    if not m:
        return None
    out = {}
    for line in m.group(1).splitlines():
        key, sep, value = line.partition(":")
        if sep:
            out[key.strip()] = value.strip()
    return out


class StrategistSkillTest(unittest.TestCase):
    def test_required_files(self):
        for rel in REQUIRED_FILES:
            self.assertTrue(os.path.isfile(os.path.join(SKILL, rel)), rel)

    def test_frontmatter(self):
        meta = frontmatter(read(os.path.join(SKILL, "SKILL.md")))
        self.assertIsNotNone(meta, "SKILL.md has no frontmatter")
        self.assertEqual(meta.get("name"), "hoi4-strategist")
        desc = meta.get("description", "")
        self.assertGreater(len(desc), 80)
        self.assertLess(len(desc), 1024)
        for word in ("plan", "run_spec", "revision", "critic"):
            self.assertIn(word, desc)

    def test_referenced_files_exist(self):
        missing = []
        for path in skill_files():
            text = read(path)
            rel_file = os.path.relpath(path, ROOT)
            for ref in REPO_PATH.findall(text):
                ref = ref.rstrip(".")
                if ref in EXPECTED_LATER:
                    continue
                if not os.path.exists(os.path.join(ROOT, ref)):
                    missing.append("%s: %s" % (rel_file, ref))
            if path.endswith(".md"):
                for link in MD_LINK.findall(text):
                    if "://" in link:
                        continue
                    target = os.path.normpath(os.path.join(os.path.dirname(path), link))
                    if os.path.relpath(target, ROOT).replace(os.sep, "/") in EXPECTED_LATER:
                        continue
                    if not os.path.exists(target):
                        missing.append("%s: link %s" % (rel_file, link))
        self.assertEqual(missing, [])

    def test_playbook_references_named_by_file_name_exist(self):
        playbook = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "references")
        here = os.path.join(SKILL, "references")
        for rel in ("references/procedure.md", "references/lessons.md", "references/rubric.md"):
            for name in set(re.findall(r"`([a-z0-9-]+\.md)`", read(os.path.join(SKILL, rel)))):
                self.assertTrue(os.path.exists(os.path.join(playbook, name)) or os.path.exists(os.path.join(here, name)),
                                "%s names %s" % (rel, name))

    def test_expected_later_are_referenced(self):
        text = "\n".join(read(p) for p in skill_files())
        for ref in EXPECTED_LATER:
            self.assertIn(ref.split("/")[-1], text, "%s listed but the skill never names it" % ref)

    def test_command_names(self):
        text = "\n".join(read(p) for p in skill_files())
        for cmd in ("python3 scripts/hoi4facts/facts.py dossier build --tag", "--out RUN/dossier",
                    "python3 scripts/validate_plan.py RUN/plan.json --strict",
                    "python3 scripts/hoi4facts/lint_plan.py RUN/plan.json --json",
                    "scripts/strategist/run.py --role critic", "research-timeline RUN/plan.json",
                    "focus-timeline <TAG> --date"):
            self.assertIn(cmd, text)

    def test_run_spec_template(self):
        spec = json.loads(read(os.path.join(SKILL, "templates", "run_spec.json")))
        for key in ("tag", "start_date", "goal", "goal_clarifications", "mode", "horizon", "dlcs", "mods",
                    "lessons"):
            self.assertIn(key, spec)
        self.assertIn(spec["mode"], ("fair", "meta"))

    def test_prompts_name_no_provider(self):
        prompts = os.path.join(SKILL, "prompts")
        names = sorted(os.listdir(prompts))
        self.assertEqual(names, ["answer.md", "critic.md", "fix.md", "revise.md", "strategist.md"])
        for name in names:
            text = read(os.path.join(prompts, name))
            self.assertIn("skills/hoi4-strategist/SKILL.md", text, name)
            self.assertIn("HOI4_LIVE_RUN", text, name)
            hits = PROVIDERS.findall(text)
            self.assertEqual(hits, [], "%s names a provider: %s" % (name, hits))

    def test_brief_template_is_short_and_generic(self):
        text = read(os.path.join(SKILL, "templates", "brief.md"))
        self.assertLessEqual(len(text.splitlines()), 60)
        for word in ("Italy", "Ethiopia", "ITA", "ETH"):
            self.assertNotIn(word, text)

    def test_rubric_rows_and_threshold(self):
        text = read(os.path.join(SKILL, "references", "rubric.md"))
        rows = re.findall(r"^\| (\d+) \|", text, re.M)
        self.assertEqual(rows, [str(i) for i in range(1, 17)])
        self.assertIn("22 of 32", text)
        self.assertNotIn("16 of 22", text)
        self.assertIn("no 0 in rows 1, 2, 7 or\n13", text)
        self.assertIn('"max": 32', text)
        for sev in ("blocker", "major", "minor"):
            self.assertIn(sev, text)
        for name in ("Force posture and XP budget", "Templates and doctrines", "Air, naval and overseas supply",
                     "Occupation, peace", "Continuous-management thresholds"):
            self.assertIn(name, text)
        skill = read(os.path.join(SKILL, "SKILL.md"))
        self.assertIn("22 of 32", skill)
        self.assertIn("0 in rows 1, 2, 7 or 13", skill)
        for rel in ("prompts/critic.md", "prompts/answer.md"):
            self.assertNotIn("of 22", read(os.path.join(SKILL, rel)), rel)

    def test_checklist_in_procedure(self):
        text = read(os.path.join(SKILL, "references", "procedure.md"))
        headings = re.findall(r"^### (\d+)\. (.+)$", text, re.M)
        self.assertEqual(headings, [(str(i), name) for i, name in enumerate(CHECKLIST, 1)])
        sections = re.split(r"^### \d+\. .+$", text, flags=re.M)[1:]
        for (num, name), body in zip(headings, sections):
            self.assertIn("Resolution", body, "item %s %s names no resolution" % (num, name))
            self.assertIn("Check", body, "item %s %s names no check" % (num, name))
            self.assertRegex(body, r"`(F|C) [a-z]|SKILL\.md step|rules-and-modes\.md|lessons\.md",
                             "item %s %s names no facts command" % (num, name))
        for word in ("executor", "player", "not applicable", "call"):
            self.assertIn(word, text.split("## Commands")[0])

    def test_pending_fact_readers_have_fallbacks(self):
        text = read(os.path.join(SKILL, "references", "procedure.md"))
        table = text.split("Until available |", 1)[1].split("\n\n", 1)[0]
        for cmd in PENDING_FACTS:
            row = [line for line in table.splitlines() if line.startswith("| `F %s" % cmd)]
            self.assertEqual(len(row), 1, cmd)
            self.assertRegex(row[0].rsplit("|", 2)[1], r"`F (grep|defines|province|state|laws|idea)|C ", cmd)
        for label in ("starting experience", "law gates"):
            self.assertIn(label, table)

    def test_checklist_in_plan_template(self):
        text = read(os.path.join(SKILL, "templates", "plan.md"))
        coverage = text.split("## 25. Coverage table", 1)[1]
        for i, name in enumerate(CHECKLIST, 1):
            self.assertIn("| %d | %s |" % (i, name), coverage, name)
        for section in ("Force posture", "Doctrine path and XP budget", "Templates", "Equipment designs and MIOs",
                        "Air", "Navy", "Diplomacy", "Intelligence", "Occupation", "Peace aims",
                        "Continuous management", "Coverage table"):
            self.assertRegex(text, r"(?m)^## \d+\. %s$" % re.escape(section), section)
        self.assertIn("## 11. Lint warnings", text)

    def test_skill_step5_hard_rule(self):
        text = read(os.path.join(SKILL, "SKILL.md"))
        step5 = text.split("**5. Domain plans", 1)[1].split("**6. Emit.**", 1)[0]
        for phrase in ("29-item checklist", "exactly one resolution", "executor key", "exact `call`",
                       "not applicable: <reason>", "coverage table", "all 29 items"):
            self.assertIn(phrase, step5)

    def test_brief_has_a_call_per_player_key(self):
        text = read(os.path.join(SKILL, "templates", "brief.md"))
        for key, tool in BRIEF_CALLS.items():
            self.assertIn("`%s {" % tool, text, key)
        schema = json.loads(read(os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")))
        player = [k for k, v in schema["properties"].items() if v.get("x-owner") == "player"
                  and "call" in ((v.get("items") or v).get("properties") or {})]
        self.assertIn("templates", player)
        for key in player:
            self.assertTrue(any(k.split(".")[0] == key for k in BRIEF_CALLS), "no brief call for " + key)

    def test_pointers(self):
        for rel in ("skills/hoi4-campaign-playbook/SKILL.md", "skills/hoi4-campaign-playbook/references/INDEX.md",
                    "AGENTS.md"):
            self.assertIn("hoi4-strategist/SKILL.md", read(os.path.join(ROOT, rel)), rel)


if __name__ == "__main__":
    unittest.main()
