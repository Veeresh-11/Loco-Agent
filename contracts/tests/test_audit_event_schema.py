"""Validate minimized audit-event examples against the v1.0 contract."""
import json
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads((ROOT / "audit-event.schema.json").read_text(encoding="utf-8"))
Draft202012Validator.check_schema(SCHEMA)
VALIDATOR = Draft202012Validator(SCHEMA, format_checker=FormatChecker())
EXAMPLE_PATH = ROOT / "examples" / "audit-event.json"

def load_example():
    return json.loads(EXAMPLE_PATH.read_text(encoding="utf-8"))

def test_audit_event_example_validates():
    errors = list(VALIDATOR.iter_errors(load_example()))
    assert not errors, "; ".join(e.message for e in errors)

def test_audit_event_rejects_unknown_top_level_fields():
    event = load_example()
    event["unexpected"] = "data"
    assert list(VALIDATOR.iter_errors(event))

def test_audit_event_details_are_scalar_only():
    event = load_example()
    event["details"]["nested"] = {"secret": "value"}
    assert list(VALIDATOR.iter_errors(event))

def test_audit_event_details_are_bounded_to_32_fields():
    event = load_example()
    event["details"] = {f"field{i}": i for i in range(33)}
    assert list(VALIDATOR.iter_errors(event))

def test_audit_event_requires_core_metadata():
    event = load_example()
    del event["outcome"]
    assert list(VALIDATOR.iter_errors(event)
