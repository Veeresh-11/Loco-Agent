"""Validate security decision examples against their published JSON Schema."""
import json
from pathlib import Path
from jsonschema import Draft202012Validator, FormatChecker

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads((ROOT / "security-decision.schema.json").read_text(encoding="utf-8"))
Draft202012Validator.check_schema(SCHEMA)
VALIDATOR = Draft202012Validator(SCHEMA, format_checker=FormatChecker())

def test_security_decision_example_validates():
    example = json.loads((ROOT / "examples" / "security-decision.json").read_text(encoding="utf-8"))
    errors = list(VALIDATOR.iter_errors(example))
    assert not errors, "; ".join(e.message for e in errors)

def test_security_decision_rejects_unknown_field():
    example = json.loads((ROOT / "examples" / "security-decision.json").read_text(encoding="utf-8"))
    example["untrustedExtra"] = "unexpected"
    assert list(VALIDATOR.iter_errors(example))

def test_approval_challenge_digest_format_is_enforced():
    example = json.loads((ROOT / "examples" / "security-decision.json").read_text(encoding="utf-8"))
    example["approvalChallenge"]["actionDigest"] = "bad"
    assert list(VALIDATOR.iter_errors(example))
