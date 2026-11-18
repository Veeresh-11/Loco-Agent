"""Validate proposal examples against the published JSON Schema contract."""
import json
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads((ROOT / "action-proposal.schema.json").read_text(encoding="utf-8"))
Draft202012Validator.check_schema(SCHEMA)
VALIDATOR = Draft202012Validator(SCHEMA, format_checker=FormatChecker())

def test_examples_validate():
    for example in (ROOT / "examples").glob("action-proposal*.json"):
        data = json.loads(example.read_text(encoding="utf-8"))
        errors = list(VALIDATOR.iter_errors(data))
        assert not errors, f"{example.name}: " + "; ".join(e.message for e in errors)

def test_unknown_action_field_rejected():
    example = json.loads((ROOT / "examples" / "action-proposal.read-file.json").read_text(encoding="utf-8"))
    example["action"]["unexpected"] = True
    assert list(VALIDATOR.iter_errors(example))

def test_unknown_top_level_field_rejected():
    example = json.loads((ROOT / "examples" / "action-proposal.read-file.json").read_text(encoding="utf-8"))
    example["unexpected"] = True
    assert list(VALIDATOR.iter_errors(example))

def test_invalid_contract_version_rejected():
    example = json.loads((ROOT / "examples" / "action-proposal.read-file.json").read_text(encoding="utf-8"))
    example["contractVersion"] = "2.0"
    assert list(VALIDATOR.iter_errors(example))
