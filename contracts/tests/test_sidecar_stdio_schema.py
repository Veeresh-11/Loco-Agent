"""Validate the Rust sidecar stdio envelope contract and examples."""
import json
from pathlib import Path
from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[1]
SCHEMA = json.loads((ROOT / "sidecar-stdio.schema.json").read_text(encoding="utf-8"))
Draft202012Validator.check_schema(SCHEMA)
VALIDATOR = Draft202012Validator(SCHEMA, registry=None)

def valid_hello():
    return {
        "protocolVersion": "1.0",
        "messageType": "hello",
        "requestId": "boot-1",
        "client": {"name": "loco-vscode-extension", "version": "0.1.0"},
    }

def test_parent_hello_example_validates():
    example = json.loads((ROOT / "examples" / "sidecar-stdio.hello.json").read_text(encoding="utf-8"))
    assert not list(VALIDATOR.iter_errors(example))

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
