"""pack.py: the strategist's work pack (schema digest, lint rule catalogue, facts outputs) without a game."""
import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from strategist import pack  # noqa: E402


def schema():
    with open(pack.SCHEMA, encoding="utf-8") as f:
        return json.load(f)


class SchemaDigest(unittest.TestCase):
    def test_every_key_has_a_file_and_an_index_row(self):
        s = schema()
        files = pack.schema_digest(s)
        for key in s["properties"]:
            self.assertIn(key + ".md", files)
            self.assertIn("| `%s` |" % key, files["INDEX.md"])

    def test_fields_types_owner_and_required(self):
        files = pack.schema_digest(schema())
        prod = files["production.md"]
        self.assertIn("- `[].equipment` string **required** [executor]", prod)
        self.assertIn("- `[].from` date", prod)
        self.assertNotIn("from.year", prod)  # dates stay one field

    def test_shared_shapes_are_linked_not_repeated(self):
        s = schema()
        files = pack.schema_digest(s)
        self.assertIn("def-predicate.md", files)
        self.assertIn("schema/def-predicate.md", files["goal.md"])
        # the predicate file carries the grammar and the atom table, not a recursive field dump
        pred = files["def-predicate.md"]
        self.assertIn("Grammar:", pred)
        self.assertIn("| `stability` |", pred)
        self.assertNotIn("all[].all[]", pred)
        # a phase override links the war shape instead of repeating it
        self.assertLess(len(files["contingencies.md"]), 8000)
        # the whole digest is far smaller than the schema it digests
        self.assertLess(sum(len(t) for t in files.values()), len(json.dumps(s)))


class LintCatalogue(unittest.TestCase):
    def test_rules_from_the_sources(self):
        rules = pack.lint_rules(pack.LINT_SOURCES)
        for rule in ("research.start_year", "gap.blocked", "plan.horizons", "army.guards"):
            self.assertIn(rule, rules)
        self.assertIn("error", rules["research.start_year"]["severity"])
        text = pack.lint_rules_md(rules)
        self.assertIn("- `gap.blocked` (warning):", text)


class Opponents(unittest.TestCase):
    def test_enemies_goal_tags_and_names_then_neighbours(self):
        spec = {"tag": "GER", "goal": "Defeat Poland and France, then the UK; keep ITA as an ally"}
        dossier = {"enemies": [], "neighbours": {"rows": [{"tag": "AUS", "name": "Austria"},
                                                          {"tag": "POL", "name": "Poland"},
                                                          {"tag": "FRA", "name": "France"}]}}
        names = {"POL": "Poland", "FRA": "France", "ITA": "Italy", "AUS": "Austria", "GER": "Germany",
                 "DDR": "Germany"}
        tags = pack.opponent_tags(spec, dossier, names)
        self.assertEqual(tags[:3], ["ITA", "FRA", "POL"])
        self.assertIn("AUS", tags)
        self.assertNotIn("GER", tags)
        self.assertNotIn("DDR", tags)  # a tag named like the player's own country is not an opponent
        self.assertNotIn("UK", tags)

    def test_enemies_at_start_come_first(self):
        tags = pack.opponent_tags({"tag": "ETH", "goal": "hold"}, {"enemies": [{"tag": "ITA"}]}, {})
        self.assertEqual(tags, ["ITA"])


class Build(unittest.TestCase):
    def test_pack_files_index_and_cache(self):
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "run_spec.json"), "w") as f:
                json.dump({"tag": "ETH", "start_date": "1936.1.1", "mode": "fair", "goal": "beat ITA"}, f)
            os.makedirs(os.path.join(d, "dossier"))
            with open(os.path.join(d, "dossier", "dossier.json"), "w") as f:
                json.dump({"enemies": [{"tag": "ITA"}]}, f)
            seen = []

            def runner(job, timeout):
                seen.append(job[1])
                return job[0], 0, "%s output\nline 2\n" % job[0]

            m = pack.build(d, runner=runner)
            self.assertEqual(m["opponents"], ["ITA"])
            names = {r["file"] for r in m["facts"]}
            for f in ("facts/laws.txt", "facts/decisions.txt", "facts/vp_ITA.txt", "facts/oob_ITA.txt",
                      "facts/research_calendar.txt", "facts/xp_army.txt"):
                self.assertIn(f, names)
            self.assertTrue(all("--tag" in argv and "ETH" in argv for argv in seen if "facts.py" in argv[1]))
            for f in ("index.md", "lint-rules.md", "schema/INDEX.md", "schema/war.md", "facts/laws.txt"):
                self.assertTrue(os.path.exists(os.path.join(d, "pack", f)), f)
            with open(os.path.join(d, "pack", "index.md"), encoding="utf-8") as f:
                index = f.read()
            self.assertIn("`python3 scripts/hoi4facts/facts.py --date 1936.1.1 --mode fair --tag ETH laws ETH`", index)
            self.assertNotIn(ROOT, index)  # no machine paths in what the model cites
            # unchanged inputs: the next build is skipped
            self.assertEqual(pack.build(d)["key"], m["key"])


if __name__ == "__main__":
    unittest.main()
