"""Documented fixes for error.log messages that the game words unhelpfully.

Each hint is one reviewed entry from the HOI4 Agent Tools list of known mistakes (the Paradox wiki's Event modding
"Common mistakes" page names every message below). They are matched by pattern, so `hoi4_logs` can say what a
folded line means and what to change. The list is deliberately short and closed: a new entry needs a wiki or
game-documentation source, and it is added in both projects together. Nothing here changes a mod or the game.
"""

import re

HINTS = (
    ("event_triggered_only_with_mtth",
     re.compile(r"Event is triggered only, but does not have a 1 base-factor", re.I),
     "An event sets is_triggered_only = yes and also mean_time_to_happen. Remove mean_time_to_happen, or remove "
     "is_triggered_only if the event should fire by itself."),
    ("event_fires_every_day",
     re.compile(r"Event is set to trigger every day", re.I),
     "An automatic event has no timer or a timer of one day or less. Make it is_triggered_only = yes and fire it "
     "from an effect or on action, add fire_only_once = yes, or give it a longer mean_time_to_happen."),
    ("event_namespace_missing",
     re.compile(r"Malformed token: [\w.]+\.\d+", re.I),
     "The event's namespace has no add_namespace line. Add add_namespace = <namespace> outside any event, before "
     "the first event that uses it."),
    ("event_numeric_id_invalid",
     re.compile(r"Failed to create id \d+ 50\. Already exists in game", re.I),
     "Two events share one internal id: a duplicate id, a non-numeric number after the namespace (all become 0), "
     "or a number of 100000 or more. End each id with a unique whole number from 0 to 99999."),
)


def hint_for(line):
    """The {id, fix} entry for an error.log line, or None."""
    for rule_id, pattern, fix in HINTS:
        if pattern.search(line):
            return {"id": rule_id, "fix": fix}
    return None
