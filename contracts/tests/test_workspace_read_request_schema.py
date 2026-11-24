import json
from pathlib import Path

from jsonschema import Draft202012Validator, FormatChecker


ROOT = Path(__file__).resolve().parents[2]


def test_workspace_read_request_schema_accepts_shared_fixture():
    schema = json.loads((ROOT / "workspace-read-request-v1.schema.json").read_text())
    fixture = json.loads((ROOT / "examples" / "workspace-read-request-v1.json").read_text())
    Draft202012Validator.check_schema(schema)
    errors = list(Draft202012Validator(schema, format_checker=FormatChecker()).iter_errors(fixture))
    assert errors == []


def test_workspace_read_request_schema_rejects_unknown_fields_and_bad_limits():
    schema = json.loads((ROOT / "workspace-read-request-v1.schema.json").read_text())
    validator = Draft202012Validator(schema)
    valid = {
        "contractVersion": "1.0",
        "workspaceId": "workspace-01",
        "relativePath": "src/main.rs",
        "maxBytes": 1024,
    }
    assert not list(validator.iter_errors(valid))
    assert list(validator.iter_errors({**valid, "execute": True}))
    assert list(validator.iter_errors({**valid, "maxBytes": 0}))
    assert list(validator.iter_errors({**valid, "maxBytes": 10485761}))
