"""A small JSON Schema (draft 7) subset validator, standard library only.

Supported keywords: $ref (local "#/definitions/..." and "#"), type (a name or a list), enum, const,
properties, patternProperties, additionalProperties (bool or schema), required, items (one schema),
minItems, maxItems, minimum, maximum, exclusiveMinimum, exclusiveMaximum, minLength, maxLength,
pattern, minProperties, maxProperties, anyOf, oneOf, allOf, not. As in draft 7, keywords next to
$ref are ignored. Annotations (title, description, default, examples, $comment, x-*) are ignored.

    errors = validate(instance, schema)        # [(path, message)], [] when valid
    problems = check_schema(schema)            # [message], [] when the schema uses only this subset
    owners(schema)                             # {top-level key: x-owner}

CLI: python3 scripts/hoi4facts/schema_check.py SCHEMA.json PLAN.json [...]  (exit 1 on errors)
"""

import json
import re
import sys

TYPES = ("null", "boolean", "integer", "number", "string", "array", "object")
KEYWORDS = {
    "$ref", "type", "enum", "const", "properties", "patternProperties", "additionalProperties",
    "required", "items", "minItems", "maxItems", "minimum", "maximum", "exclusiveMinimum",
    "exclusiveMaximum", "minLength", "maxLength", "pattern", "minProperties", "maxProperties",
    "anyOf", "oneOf", "allOf", "not",
}
ANNOTATIONS = {"$schema", "$id", "$comment", "title", "description", "default", "examples", "definitions"}
DRAFT7 = ("http://json-schema.org/draft-07/schema#", "http://json-schema.org/draft-07/schema")


def _is_type(value, name):
    if name == "null":
        return value is None
    if name == "boolean":
        return isinstance(value, bool)
    if name == "integer":
        return isinstance(value, int) and not isinstance(value, bool) or (
            isinstance(value, float) and value.is_integer())
    if name == "number":
        return isinstance(value, (int, float)) and not isinstance(value, bool)
    if name == "string":
        return isinstance(value, str)
    if name == "array":
        return isinstance(value, list)
    if name == "object":
        return isinstance(value, dict)
    return False


def _equal(a, b):
    # JSON equality: true is not 1
    if isinstance(a, bool) or isinstance(b, bool):
        return isinstance(a, bool) and isinstance(b, bool) and a == b
    return a == b


def _resolve(ref, root):
    if ref == "#":
        return root
    if not ref.startswith("#/"):
        raise ValueError("only local $ref is supported: %s" % ref)
    node = root
    for part in ref[2:].split("/"):
        part = part.replace("~1", "/").replace("~0", "~")
        node = node[part]
    return node


def _join(path, key):
    if isinstance(key, int):
        return "%s[%d]" % (path, key)
    return key if path == "" else "%s.%s" % (path, key)


def validate(instance, schema, root=None, path=""):
    """Errors of `instance` against `schema` as [(path, message)]."""
    root = schema if root is None else root
    errors = []
    if schema is True or schema == {}:
        return errors
    if schema is False:
        return [(path or "$", "no value is allowed here")]
    if "$ref" in schema:
        return validate(instance, _resolve(schema["$ref"], root), root, path)
    where = path or "$"
    if "type" in schema:
        names = schema["type"] if isinstance(schema["type"], list) else [schema["type"]]
        if not any(_is_type(instance, n) for n in names):
            return [(where, "expected %s, got %s" % ("|".join(names), _json_type(instance)))]
    if "enum" in schema and not any(_equal(instance, e) for e in schema["enum"]):
        errors.append((where, "%s is not one of %s" % (json.dumps(instance), json.dumps(schema["enum"]))))
    if "const" in schema and not _equal(instance, schema["const"]):
        errors.append((where, "must be %s" % json.dumps(schema["const"])))
    if isinstance(instance, str):
        if "minLength" in schema and len(instance) < schema["minLength"]:
            errors.append((where, "shorter than %d" % schema["minLength"]))
        if "maxLength" in schema and len(instance) > schema["maxLength"]:
            errors.append((where, "longer than %d" % schema["maxLength"]))
        if "pattern" in schema and not re.search(schema["pattern"], instance):
            shown = schema["pattern"] if len(schema["pattern"]) <= 60 else schema["pattern"][:57] + "..."
            errors.append((where, "%s does not match %s" % (json.dumps(instance), shown)))
    if _is_type(instance, "number"):
        if "minimum" in schema and instance < schema["minimum"]:
            errors.append((where, "%s < minimum %s" % (instance, schema["minimum"])))
        if "maximum" in schema and instance > schema["maximum"]:
            errors.append((where, "%s > maximum %s" % (instance, schema["maximum"])))
        if "exclusiveMinimum" in schema and instance <= schema["exclusiveMinimum"]:
            errors.append((where, "%s <= exclusiveMinimum %s" % (instance, schema["exclusiveMinimum"])))
        if "exclusiveMaximum" in schema and instance >= schema["exclusiveMaximum"]:
            errors.append((where, "%s >= exclusiveMaximum %s" % (instance, schema["exclusiveMaximum"])))
    if isinstance(instance, list):
        if "minItems" in schema and len(instance) < schema["minItems"]:
            errors.append((where, "fewer than %d items" % schema["minItems"]))
        if "maxItems" in schema and len(instance) > schema["maxItems"]:
            errors.append((where, "more than %d items" % schema["maxItems"]))
        if "items" in schema:
            for i, item in enumerate(instance):
                errors += validate(item, schema["items"], root, _join(path, i))
    if isinstance(instance, dict):
        for key in schema.get("required", []):
            if key not in instance:
                errors.append((where, "missing required key %r" % key))
        if "minProperties" in schema and len(instance) < schema["minProperties"]:
            errors.append((where, "fewer than %d keys" % schema["minProperties"]))
        if "maxProperties" in schema and len(instance) > schema["maxProperties"]:
            errors.append((where, "more than %d keys" % schema["maxProperties"]))
        props = schema.get("properties", {})
        patterns = schema.get("patternProperties", {})
        extra = schema.get("additionalProperties", True)
        for key, value in instance.items():
            sub = _join(path, key)
            matched = False
            if key in props:
                matched = True
                errors += validate(value, props[key], root, sub)
            for pat, pschema in patterns.items():
                if re.search(pat, key):
                    matched = True
                    errors += validate(value, pschema, root, sub)
            if not matched:
                if extra is False:
                    errors.append((sub, "unknown key %r" % key))
                elif isinstance(extra, dict):
                    errors += validate(value, extra, root, sub)
    for sub in schema.get("allOf", []):
        errors += validate(instance, sub, root, path)
    if "anyOf" in schema:
        results = [validate(instance, sub, root, path) for sub in schema["anyOf"]]
        if all(results):
            errors += _best(results, where, "anyOf")
    if "oneOf" in schema:
        results = [validate(instance, sub, root, path) for sub in schema["oneOf"]]
        passing = sum(1 for r in results if not r)
        if passing == 0:
            errors += _best(results, where, "oneOf")
        elif passing > 1:
            errors.append((where, "matches %d oneOf branches, expected exactly one" % passing))
    if "not" in schema and not validate(instance, schema["not"], root, path):
        errors.append((where, "must not match the 'not' schema"))
    return errors


def _best(results, where, word):
    fewest = min(results, key=len)
    return [(where, "matches no %s branch; nearest: %s" % (word, "; ".join("%s: %s" % e for e in fewest)))]


def _json_type(value):
    for name in ("null", "boolean", "integer", "number", "string", "array", "object"):
        if _is_type(value, name):
            return name
    return type(value).__name__


def check_schema(schema, root=None, path="#"):
    """Problems that make `schema` fall outside the supported draft-7 subset (or broken)."""
    root = schema if root is None else root
    problems = []
    if path == "#" and schema.get("$schema") not in DRAFT7:
        problems.append("#: $schema must be the draft-07 URI")
    if isinstance(schema, bool):
        return problems
    if not isinstance(schema, dict):
        return ["%s: a schema must be an object or a boolean" % path]
    for key in schema:
        if key not in KEYWORDS and key not in ANNOTATIONS and not key.startswith("x-"):
            problems.append("%s: unsupported keyword %r" % (path, key))
    if "$ref" in schema:
        try:
            _resolve(schema["$ref"], root)
        except (KeyError, ValueError, TypeError):
            problems.append("%s: $ref %s does not resolve" % (path, schema["$ref"]))
    if "type" in schema:
        names = schema["type"] if isinstance(schema["type"], list) else [schema["type"]]
        for n in names:
            if n not in TYPES:
                problems.append("%s: unknown type %r" % (path, n))
    if "pattern" in schema:
        try:
            re.compile(schema["pattern"])
        except re.error as e:
            problems.append("%s: bad pattern: %s" % (path, e))
    for name in ("properties", "patternProperties", "definitions"):
        for key, sub in schema.get(name, {}).items():
            if name == "patternProperties":
                try:
                    re.compile(key)
                except re.error as e:
                    problems.append("%s/patternProperties: bad pattern %r: %s" % (path, key, e))
            problems += check_schema(sub, root, "%s/%s/%s" % (path, name, key))
    if isinstance(schema.get("additionalProperties"), dict):
        problems += check_schema(schema["additionalProperties"], root, path + "/additionalProperties")
    if "items" in schema:
        if not isinstance(schema["items"], (dict, bool)):
            problems.append("%s: items must be one schema (tuple form is not supported)" % path)
        else:
            problems += check_schema(schema["items"], root, path + "/items")
    for name in ("anyOf", "oneOf", "allOf"):
        for i, sub in enumerate(schema.get(name, [])):
            problems += check_schema(sub, root, "%s/%s/%d" % (path, name, i))
    if "not" in schema:
        problems += check_schema(schema["not"], root, path + "/not")
    required = schema.get("required", [])
    if not isinstance(required, list) or not all(isinstance(r, str) for r in required):
        problems.append("%s: required must be a list of strings" % path)
    elif schema.get("additionalProperties") is False and "properties" in schema:
        for r in required:
            if r not in schema["properties"]:
                problems.append("%s: required %r is not among the properties" % (path, r))
    return problems


def owners(schema):
    """{top-level key: x-owner} of a plan schema."""
    return {k: v.get("x-owner") for k, v in schema.get("properties", {}).items()}


def by_top_key(errors):
    """Group (path, message) errors by their top-level plan key."""
    out = {}
    for path, message in errors:
        top = re.split(r"[.\[]", path, maxsplit=1)[0] or "$"
        out.setdefault(top, []).append((path, message))
    return out


def main(argv):
    if len(argv) < 3:
        print(__doc__.strip().splitlines()[-1])
        return 2
    with open(argv[1], encoding="utf-8") as f:
        schema = json.load(f)
    bad = 0
    for plan_path in argv[2:]:
        with open(plan_path, encoding="utf-8") as f:
            errors = validate(json.load(f), schema)
        print("%s: %d error(s)" % (plan_path, len(errors)))
        for path, message in errors:
            print("  %s: %s" % (path, message))
        bad += bool(errors)
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
