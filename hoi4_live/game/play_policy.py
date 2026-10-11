"""Portable, fail-closed campaign policy, separate from the legacy QA/test mode.

The adjacent JSON is the reviewable source of modes, tool/action coverage and
prohibitions. Call ``authorize`` after schema validation and before any UI input,
including background/direct handler calls. Never accept ReviewEvidence from MCP
arguments: only an independently verified server-side adapter may produce it.

Normal land orders can be reviewed against fresh trusted own-army, public-map
and complete template observations. This checks concrete house constraints, not
strategic intent or the absence of every unknown engine exploit. Unsupported
operations need a separate reviewed adapter. META never waives universal bans.
"""

from dataclasses import dataclass
import copy
import hashlib
import json
import logging
import math
from pathlib import Path
import time
import threading
import uuid
from typing import FrozenSet

from ..errors import ModeRefused


log = logging.getLogger("hoi4_live.play_policy")
POLICY_PATH = Path(__file__).with_name("play_policy.json")


def request_digest(tool_name, arguments):
    """Bind a review to the exact normalized, schema-validated request."""
    value = json.dumps([tool_name, arguments], sort_keys=True, separators=(",", ":"),
                       ensure_ascii=False, allow_nan=False).encode("utf-8")
    return hashlib.sha256(value).hexdigest()


@dataclass(frozen=True)
class ReviewEvidence:
    """Trusted adapter output, not caller-provided declarations.

    ``checks`` name independently verified conclusions. A reviewer must inspect
    complete proposed templates / operation history, not just an ``add`` list or
    a requested intent. Land-order evidence can be produced by the deterministic
    reviewer below when a trusted live observation adapter supplies its inputs.
    ``session_id`` prevents using a prior campaign's evidence for a new session.
    """

    session_id: str
    policy_sha256: str
    request_sha256: str
    observed_monotonic: float
    source: str
    checks: FrozenSet[str]
    evidence_id: str


def _load_policy(path):
    try:
        raw = Path(path).read_bytes()
        policy = json.loads(raw.decode("utf-8"))
        if (policy["version"] != 1 or policy["modes"] != ["fair", "meta"]
                or policy["default_mode"] != "fair" or policy["showcase_mode"] != "fair"
                or not isinstance(policy["tools"], dict)
                or not policy["prohibited_in_both"] or not policy["fair_prohibited"]
                or not 0 < policy["evidence_max_age_seconds"] <= 60):
            raise ValueError("unsupported or incomplete policy")
        if not all(isinstance(policy[key], str) and policy[key]
                   for key in ("id", "enforcement_limits")):
            raise ValueError("missing policy identity or limitations")
        if (not isinstance(policy["showcase_modes"], dict) or not policy["showcase_modes"]
                or any(not isinstance(tag, str) or mode not in policy["modes"]
                       for tag, mode in policy["showcase_modes"].items())):
            raise ValueError("invalid showcase mode mapping")
        for rule in policy["tools"].values():
            if (not rule["actions"] or not set(rule["actions"].values()).issubset(
                    {"allow", "deny", "fair_review", "resume_review"})):
                raise ValueError("invalid action rule")
        for rule in policy["argument_denials"]:
            if rule["operator"] not in ("equals", "not_equals", "truthy", "in", "contains"):
                raise ValueError("unknown argument rule")
            if not rule["tools"] or not rule["field"] or not rule["reason"]:
                raise ValueError("incomplete argument rule")
        for key in ("review_sources", "fair_review_requirements", "resume_review_requirements"):
            if not policy[key] or not all(isinstance(x, str) and x for x in policy[key]):
                raise ValueError("incomplete review requirements")
        land = policy["fair_land_rules"]
        if (not isinstance(land, dict) or not land["actions"]
                or not set(land["actions"]).issubset({"front", "offensive", "spearhead", "attack", "batch", "execute"})
                or not land["line_roles"] or any(not isinstance(role, str) for role in land["line_roles"])
                or any(type(land[key]) is not int or land[key] < 1 for key in
                       ("max_armies_per_call", "max_distinct_targets_per_army_per_day",
                        "min_line_battalions", "max_line_battalions"))
                or land["max_armies_per_call"] > 40 or land["min_line_battalions"] > land["max_line_battalions"]
                or land["max_leg_infantry_or_cavalry_with_line_armor"] != 0):
            raise ValueError("invalid deterministic land rules")
        if not isinstance(policy["allowed_launch_arguments"], list):
            raise ValueError("invalid launch arguments")
        return policy, hashlib.sha256(raw).hexdigest()
    except (OSError, ValueError, KeyError, TypeError) as exc:
        raise ModeRefused("campaign play policy is missing or invalid; refusing campaign actions",
                          {"error": "play_policy_invalid"}) from exc


@dataclass(frozen=True)
class LandObservation:
    """Trusted adapter snapshot, never an MCP argument or an unfiltered save.

    Armies: {id, name, owner, template_ids, order_types}. States: {id, visible,
    controller}. Templates: {id, owner, complete, line:[{type, role, count}]}.
    Roles must come from installed unit definitions, not model guesses. Only
    own armies/templates and public map controllers may be included. A provider
    must bind selected UI names to stable IDs and perform post-action readback.
    """
    session_id: str
    tag: str
    game_day: int
    observed_monotonic: float
    source: str
    armies: tuple
    states: tuple
    templates: tuple


class PlayPolicy:
    """One immutable FAIR/META selection per session, with bounded audit history.

    ``audit`` can persist events to the external run manifest/journal. Exceptions
    from it propagate before action permission is returned: failed audit is not
    silently treated as successful logging. The normal logger uses stderr/files.
    """

    def __init__(self, mode="fair", *, session_id="", showcase=False, audit=None,
                 policy_path=POLICY_PATH, monotonic=None):
        policy, digest = _load_policy(policy_path)
        if mode not in policy["modes"]:
            raise ModeRefused("play mode must be fair or meta", {"error": "invalid_play_mode"})
        if showcase is True:
            required_mode = policy["showcase_mode"]
        elif showcase is False:
            required_mode = None
        elif isinstance(showcase, str) and showcase in policy["showcase_modes"]:
            required_mode = policy["showcase_modes"][showcase]
        else:
            raise ModeRefused("showcase must name a reviewed country tag or be a boolean",
                              {"error": "unreviewed_showcase"})
        if required_mode is not None and mode != required_mode:
            raise ModeRefused("this recorded showcase requires {} play mode".format(required_mode),
                              {"error": "showcase_mode_mismatch", "required_play_mode": required_mode,
                               "showcase": showcase})
        self._policy = policy
        self._mode = mode
        self._digest = digest
        self._session_id = str(session_id)
        self._showcase = showcase
        self._audit = audit
        self._monotonic = monotonic or time.monotonic
        self._events = []
        self._used_evidence = set()
        self._land_day = None
        self._land_tag = None
        self._land_targets = {}
        self._mutex = threading.RLock()
        self._record("session_selected", True, "session_play_mode")

    @property
    def mode(self):
        return self._mode

    @property
    def sha256(self):
        return self._digest

    def summary(self):
        return {"play_mode": self.mode, "policy_id": self._policy["id"],
                "policy_version": self._policy["version"], "policy_sha256": self.sha256,
                "showcase": self._showcase, "enforcement_limits": self._policy["enforcement_limits"]}

    def audit_events(self):
        return [dict(event) for event in self._events]

    def _record(self, tool, allowed, reason, action=None, request=None):
        event = {"event": "play_policy", "session_id": self._session_id,
                 "play_mode": self.mode, "policy_sha256": self.sha256,
                 "policy_id": self._policy["id"], "time": time.time(),
                 "tool": tool, "action": action, "allowed": bool(allowed), "reason": reason}
        if request:
            event["request_sha256"] = request
        if self._audit is not None:
            self._audit(dict(event))
        self._events.append(event)
        del self._events[:-256]
        log.info("play policy %s", json.dumps(event, sort_keys=True))
        return event

    def _refuse(self, tool, reason, action=None, request=None, requirements=None):
        self._record(tool, False, reason, action, request)
        details = {"error": "play_policy_refused", "reason": reason,
                   "play_mode": self.mode, "policy_sha256": self.sha256}
        if requirements:
            details["required_verified_checks"] = list(requirements)
        raise ModeRefused("{} refused by {} play policy: {}".format(tool, self.mode, reason), details)

    def validate_launch_arguments(self, arguments):
        """Also check configured launch flags, which are outside tool arguments."""
        if (not isinstance(arguments, (list, tuple)) or any(
                arg not in self._policy["allowed_launch_arguments"] for arg in arguments)):
            self._refuse("hoi4_launch", "unreviewed_launch_arguments")

    def _review(self, evidence, digest, requirements):
        if not isinstance(evidence, ReviewEvidence) or not self._session_id:
            return False
        if (not isinstance(evidence.observed_monotonic, (int, float))
                or not math.isfinite(evidence.observed_monotonic)
                or not isinstance(evidence.evidence_id, str)):
            return False
        age = self._monotonic() - evidence.observed_monotonic
        return (evidence.session_id == self._session_id and evidence.policy_sha256 == self.sha256
                and evidence.request_sha256 == digest
                and evidence.source in self._policy["review_sources"]
                and bool(evidence.evidence_id)
                and evidence.evidence_id not in self._used_evidence
                and 0 <= age <= self._policy["evidence_max_age_seconds"]
                and isinstance(evidence.checks, frozenset)
                and set(requirements).issubset(evidence.checks))

    def authorize(self, tool_name, arguments, *, test_only=False, evidence=None, observation=None):
        """Approve a reviewed action or raise ModeRefused before side effects.

        Caller must independently apply own-country visibility and ownership
        guards. Policy reviews cannot waive test-only or universal restrictions.
        Unknown tool/action pairs are refused even if registered elsewhere.
        """
        with self._mutex:
            return self._authorize(tool_name, arguments, test_only=test_only, evidence=evidence,
                                   observation=observation)

    def _authorize(self, tool_name, arguments, *, test_only=False, evidence=None, observation=None):
        if not isinstance(arguments, dict):
            self._refuse(tool_name, "arguments_must_be_validated_object")
        digest = request_digest(tool_name, arguments)
        if test_only:
            self._refuse(tool_name, "test_only_tool", request=digest)
        rule = self._policy["tools"].get(tool_name)
        if rule is None:
            self._refuse(tool_name, "unreviewed_tool", request=digest)
        action = arguments.get("action", rule["default_action"]) or "_call"
        if not isinstance(action, str):
            self._refuse(tool_name, "unreviewed_action", request=digest)
        verdict = rule["actions"].get(action)
        if verdict is None:
            self._refuse(tool_name, "unreviewed_action", action, digest)
        if verdict == "deny":
            self._refuse(tool_name, "prohibited_or_unverifiable_action", action, digest)
        for denial in self._policy["argument_denials"]:
            if "*" not in denial["tools"] and tool_name not in denial["tools"]:
                continue
            value = arguments.get(denial["field"], denial.get("default"))
            op = denial["operator"]
            expected = denial.get("value")
            match = ((op == "truthy" and bool(value)) or (op == "equals" and value == expected)
                     or (op == "not_equals" and value != expected)
                     or (op == "in" and value in expected)
                     or (op == "contains" and value is not None and expected in value))
            if match:
                self._refuse(tool_name, denial["reason"], action, digest)
        requirements = None
        if verdict == "resume_review":
            requirements = self._policy["resume_review_requirements"]
        elif verdict == "fair_review" and self.mode == "fair":
            requirements = self._policy["fair_review_requirements"]
        pending_land = None
        if verdict == "fair_review" and self.mode == "fair" and observation is not None:
            evidence, pending_land = self._review_land(tool_name, arguments, observation, digest)
        if requirements and not self._review(evidence, digest, requirements):
            self._refuse(tool_name, "verified_review_unavailable", action, digest, requirements)
        event = self._record(tool_name, True, "reviewed_action" if requirements else "policy_allowed",
                             action, digest)
        if requirements:
            self._used_evidence.add(evidence.evidence_id)
        if pending_land is not None:
            self._land_day, self._land_tag, self._land_targets = pending_land
        return event

    def _review_land(self, tool, arguments, observation, digest):
        """Pure proposed review; history commits only after the audit succeeds.

        Identical retry targets do not consume another churn allowance. History
        counts authorized targets, including failures, conservatively; it never
        mistakes permission for verified execution or a completed standing plan.
        """
        def refuse(reason):
            self._refuse(tool, reason, arguments.get('action'), digest)

        obs = copy.deepcopy(observation)
        rules = self._policy.get('fair_land_rules', {})
        if not isinstance(obs, LandObservation) or not self._session_id:
            refuse('trusted_land_observation_required')
        if (obs.session_id != self._session_id or obs.source not in self._policy['review_sources']
                or not isinstance(obs.tag, str) or len(obs.tag) != 3
                or type(obs.game_day) is not int or obs.game_day < 0
                or not isinstance(obs.observed_monotonic, (int, float))
                or not math.isfinite(obs.observed_monotonic)
                or not 0 <= self._monotonic() - obs.observed_monotonic <= self._policy['evidence_max_age_seconds']):
            refuse('land_observation_untrusted_or_stale')
        if (self._land_tag is not None and self._land_tag != obs.tag
                or self._land_day is not None and obs.game_day < self._land_day):
            refuse('land_campaign_identity_or_timeline_changed')
        if tool != 'hoi4_orders' or arguments.get('action') not in rules.get('actions', []):
            refuse('land_action_adapter_unavailable')
        if arguments.get('tag') not in (None, obs.tag):
            refuse('land_country_mismatch')
        if (any(not isinstance(rows, tuple) for rows in (obs.armies, obs.states, obs.templates))
                or len(obs.armies) > 256 or len(obs.states) > 2048 or len(obs.templates) > 512):
            refuse('land_observation_incomplete')

        def indexed(rows, owner=False):
            result = {}
            for row in rows:
                if not isinstance(row, dict):
                    refuse('land_observation_incomplete')
                ident = row.get('id')
                if type(ident) is not int or ident <= 0 or ident in result:
                    refuse('land_id_unknown_or_ambiguous')
                if owner and row.get('owner') != obs.tag:
                    refuse('land_foreign_units_excluded')
                result[ident] = row
            return result
        armies = indexed(obs.armies, True)
        states = indexed(obs.states)
        templates = indexed(obs.templates, True)
        action = arguments['action']
        rows = arguments.get('orders') if action == 'batch' else [arguments]
        if not isinstance(rows, list) or not 1 <= len(rows) <= rules['max_armies_per_call']:
            refuse('land_order_batch_invalid')
        targets = ({key: set(value) for key, value in self._land_targets.items()}
                   if self._land_day == obs.game_day else {})
        seen_armies = set()
        for row in rows:
            if not isinstance(row, dict) or row.get('army_index') is not None:
                refuse('land_army_stable_binding_required')
            name = row.get('army')
            matches = [army for army in armies.values() if isinstance(name, str) and army.get('name') == name]
            if len(matches) != 1:
                refuse('land_army_unknown_or_ambiguous')
            army = matches[0]
            if army['id'] in seen_armies:
                refuse('land_duplicate_army_order')
            seen_armies.add(army['id'])
            tids = army.get('template_ids')
            if not isinstance(tids, list) or not tids or any(type(t) is not int or t not in templates for t in tids):
                refuse('land_templates_unknown')
            for tid in tids:
                template = templates[tid]
                line = template.get('line')
                if template.get('complete') is not True or not isinstance(line, list) or not line:
                    refuse('land_templates_unknown')
                roles, total = set(), 0
                for battalion in line:
                    if (not isinstance(battalion, dict) or not isinstance(battalion.get('type'), str)
                            or not battalion['type'] or battalion.get('role') not in rules['line_roles']
                            or type(battalion.get('count')) is not int or battalion['count'] <= 0):
                        refuse('land_templates_unknown')
                    roles.add(battalion['role'])
                    total += battalion['count']
                if not rules['min_line_battalions'] <= total <= rules['max_line_battalions']:
                    refuse('land_template_size_limit')
                if ('armor' in roles and roles.intersection({'leg_infantry', 'cavalry'})
                        and rules['max_leg_infantry_or_cavalry_with_line_armor'] == 0):
                    refuse('fair_space_marines')
            if action == 'execute':
                orders = army.get('order_types')
                if not isinstance(orders, list) or not orders or any(
                        order not in ('front_line', 'offensive', 'spearhead', 'fallback') for order in orders):
                    refuse('land_existing_orders_unknown_or_special')
                continue
            target_fields = ('front_target', 'target') if action in ('attack', 'batch') else ('target',)
            for field in target_fields:
                value = row.get(field)
                if not isinstance(value, str) or not value.isascii() or not value.isdecimal() or int(value) <= 0:
                    refuse('land_state_id_required')
                state = states.get(int(value))
                if (state is None or state.get('visible') is not True
                        or not isinstance(state.get('controller'), str) or len(state['controller']) != 3):
                    refuse('land_target_unknown_or_not_visible')
                targets.setdefault(army['id'], set()).add(int(value))
            if len(targets[army['id']]) > rules['max_distinct_targets_per_army_per_day']:
                refuse('fair_land_target_churn_limit')
        evidence = ReviewEvidence(self._session_id, self.sha256, digest, obs.observed_monotonic,
                                  obs.source, frozenset(self._policy['fair_review_requirements']), uuid.uuid4().hex)
        return evidence, (obs.game_day, obs.tag, targets)
