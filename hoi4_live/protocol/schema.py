"""A small JSON-schema validator for tool arguments (the subset the tool schemas use).

Supported keywords: type (string or list), properties, required, additionalProperties (bool or schema), enum,
const, default, minimum, maximum, exclusiveMinimum, exclusiveMaximum, minLength, maxLength, pattern, items,
minItems, maxItems, uniqueItems, oneOf, anyOf, description/title/examples (ignored).

validate(schema, value) returns the value with defaults filled in, or raises ValidationError whose `field`
names the offending argument path (for example `steps[2].args.tag`).
"""

import copy
import re

from ..errors import ValidationError

_TYPES = {
    "string": lambda v: isinstance(v, str),
    "integer": lambda v: isinstance(v, int) and not isinstance(v, bool),
    "number": lambda v: isinstance(v, (int, float)) and not isinstance(v, bool),
    "boolean": lambda v: isinstance(v, bool),
    "object": lambda v: isinstance(v, dict),
    "array": lambda v: isinstance(v, list),
    "null": lambda v: v is None,
}


def _type_name(v):
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "boolean"
    if isinstance(v, int):
        return "integer"
    if isinstance(v, float):
        return "number"
    if isinstance(v, str):
        return "string"
    if isinstance(v, list):
        return "array"
    if isinstance(v, dict):
        return "object"
    return type(v).__name__


def _join(path, key):
    if isinstance(key, int):
        return f"{path}[{key}]"
    return f"{path}.{key}" if path else str(key)


def validate(schema, value, path="arguments"):
    if not schema:
        return value
    if "oneOf" in schema or "anyOf" in schema:
        options = schema.get("oneOf") or schema.get("anyOf")
        errors = []
        for opt in options:
            try:
                return validate(opt, value, path)
            except ValidationError as e:
                errors.append(str(e))
        raise ValidationError(path, "does not match any allowed form: " + " | ".join(errors))

    t = schema.get("type")
    if t is not None:
        types = t if isinstance(t, list) else [t]
        if not any(_TYPES[x](value) for x in types):
            raise ValidationError(path, f"expected {' or '.join(types)}, got {_type_name(value)}")
        # integers are acceptable numbers; floats with integral value are acceptable integers
    if "const" in schema and value != schema["const"]:
        raise ValidationError(path, f"must be {schema['const']!r}")
    if "enum" in schema and value not in schema["enum"]:
        raise ValidationError(path, f"must be one of {schema['enum']}, got {value!r}")

    if isinstance(value, (int, float)) and not isinstance(value, bool):
        if "minimum" in schema and value < schema["minimum"]:
            raise ValidationError(path, f"must be >= {schema['minimum']}, got {value}")
        if "maximum" in schema and value > schema["maximum"]:
            raise ValidationError(path, f"must be <= {schema['maximum']}, got {value}")
        if "exclusiveMinimum" in schema and value <= schema["exclusiveMinimum"]:
            raise ValidationError(path, f"must be > {schema['exclusiveMinimum']}, got {value}")
        if "exclusiveMaximum" in schema and value >= schema["exclusiveMaximum"]:
            raise ValidationError(path, f"must be < {schema['exclusiveMaximum']}, got {value}")

    if isinstance(value, str):
        if "minLength" in schema and len(value) < schema["minLength"]:
            raise ValidationError(path, f"must have at least {schema['minLength']} characters")
        if "maxLength" in schema and len(value) > schema["maxLength"]:
            raise ValidationError(path, f"must have at most {schema['maxLength']} characters")
        if "pattern" in schema and not re.search(schema["pattern"], value):
            raise ValidationError(path, f"must match pattern {schema['pattern']}, got {value!r}")

    if isinstance(value, list):
        if "minItems" in schema and len(value) < schema["minItems"]:
            raise ValidationError(path, f"must have at least {schema['minItems']} items")
        if "maxItems" in schema and len(value) > schema["maxItems"]:
            raise ValidationError(path, f"must have at most {schema['maxItems']} items")
        if schema.get("uniqueItems"):
            seen = []
            for v in value:
                if v in seen:
                    raise ValidationError(path, f"items must be unique, {v!r} repeats")
                seen.append(v)
        items = schema.get("items")
        if items:
            value = [validate(items, v, _join(path, i)) for i, v in enumerate(value)]

    if isinstance(value, dict):
        props = schema.get("properties", {})
        out = {}
        for k, v in value.items():
            if k in props:
                out[k] = validate(props[k], v, _join(path, k))
            else:
                extra = schema.get("additionalProperties", True)
                if extra is False:
                    allowed = ", ".join(sorted(props)) or "none"
                    raise ValidationError(_join(path, k), f"unknown argument (allowed: {allowed})")
                out[k] = validate(extra, v, _join(path, k)) if isinstance(extra, dict) else v
        for k in schema.get("required", []):
            if k not in out:
                raise ValidationError(_join(path, k), "is required")
        for k, sub in props.items():
            if k not in out and "default" in sub:
                out[k] = copy.deepcopy(sub["default"])
        value = out
    return value


def check_schema(schema, path="schema"):
    """Sanity-check a tool schema at registration time (catches typos in our own definitions)."""
    known = {"type", "properties", "required", "additionalProperties", "enum", "const", "default", "minimum",
             "maximum", "exclusiveMinimum", "exclusiveMaximum", "minLength", "maxLength", "pattern", "items",
             "minItems", "maxItems", "uniqueItems", "oneOf", "anyOf", "description", "title", "examples"}
    for k in schema:
        if k not in known:
            raise ValueError(f"{path}: unsupported schema keyword {k!r}")
    t = schema.get("type")
    for x in (t if isinstance(t, list) else [t] if t else []):
        if x not in _TYPES:
            raise ValueError(f"{path}: unknown type {x!r}")
    for k, sub in schema.get("properties", {}).items():
        check_schema(sub, f"{path}.properties.{k}")
        if "default" in sub:
            try:
                validate(sub, sub["default"], f"{path}.properties.{k}.default")
            except ValidationError as e:
                raise ValueError(str(e))
    for k in schema.get("required", []):
        if k not in schema.get("properties", {}):
            raise ValueError(f"{path}: required {k!r} is not a property")
    if isinstance(schema.get("items"), dict):
        check_schema(schema["items"], f"{path}.items")
    for k in ("oneOf", "anyOf"):
        for i, sub in enumerate(schema.get(k, [])):
            check_schema(sub, f"{path}.{k}[{i}]")
    if isinstance(schema.get("additionalProperties"), dict):
        check_schema(schema["additionalProperties"], f"{path}.additionalProperties")
