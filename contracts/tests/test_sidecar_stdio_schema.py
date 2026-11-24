"""Validate the Rust sidecar stdio envelope contract and examples."""
import json
from pathlib import Path
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads((ROOT / "sidecar-stdio.schema.json").read_text(encoding="utf-8"))
ACTION_SCHEMA = json.loads((ROOT / "action-proposal.schema.json").read_text(encoding="utf-8"))
REGISTRY = Registry().with_resource(
    ACTION_SCHEMA["$id"], Resource.from_contents(ACTION_SCHEMA)
)
Draft202012Validator.check_schema(SCHEMA)
VALIDATOR = Draft202012Validator(SCHEMA, registry=REGISTRY)

def valid_hello():
    return {
        "protocolVersion": "1.0",
        "messageType": "hello",
        "requestId": "boot-1",
        "client": {"name": "loco-vscode-extension", "version": "0.1.0"},
    }

def valid_proposal():
    return {
        "contractVersion": "1.0",
        "proposalId": "proposal-1",
        "taskId": "task-1",
        "createdAt": "2026-11-24T00:00:00Z",
        "action": {"type": "read_file", "path": "README.md", "maxBytes": 1024},
    }

def test_parent_hello_example_validates():
    example = json.loads((ROOT / "examples" / "sidecar-stdio.hello.json").read_text(encoding="utf-8"))
    assert not list(VALIDATOR.iter_errors(example))

def test_evaluate_request_validates_nested_proposal():
    message = {
        "protocolVersion": "1.0",
        "messageType": "evaluate",
        "requestId": "eval-1",
        "proposal": valid_proposal(),
    }
    assert not list(VALIDATOR.iter_errors(message))

def test_unknown_message_type_rejected():
    message = valid_hello()
    message["messageType"] = "execute"
    assert list(VALIDATOR.iter_errors(message))

def test_unknown_envelope_field_rejected():
    message = valid_hello()
    message["isTrusted"] = True
    assert list(VALIDATOR.iter_errors(message))

def test_protocol_downgrade_rejected():
    message = valid_hello()
    message["protocolVersion"] = "0.9"
    assert list(VALIDATOR.iter_errors(message))

def test_invalid_request_id_rejected():
    message = valid_hello()
    message["requestId"] = "not a safe id"
    assert list(VALIDATOR.iter_errors(message))

def test_client_cannot_claim_authority():
    message = valid_hello()
    message["client"]["trusted"] = True
    assert list(VALIDATOR.iter_errors(message))

def test_evaluate_rejects_unknown_nested_proposal_field():
    message = {
        "protocolVersion": "1.0",
        "messageType": "evaluate",
        "requestId": "eval-1",
        "proposal": valid_proposal(),
    }
    message["proposal"]["trusted"] = True
    assert list(VALIDATOR.iter_errors(message))
