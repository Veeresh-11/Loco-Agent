"""Vector checks for canonical action digest v1."""
import unittest
from canonical_action_digest import canonical_action_digest

class DigestVectors(unittest.TestCase):
    def test_read_file(self):
        self.assertEqual(canonical_action_digest({"type":"read_file","path":"src/main.rs","max_bytes":1024}),
          "87cc9be51307092dd5c6d9203de172422cabdc506f78bcb224b07f39d15022a7")

    def test_propose_patch(self):
        self.assertEqual(canonical_action_digest({"type":"propose_patch","path":"src/lib.rs","patch":"diff --git a b","expected_target_sha256":"a"*64}),
          "d8ae37db445a5bf6b34239e8032fa0abb239e130c026ace019ecdaaf232de192")

    def test_run_command(self):
        self.assertEqual(canonical_action_digest({"type":"run_command","executable":"cargo","args":["test","--lib"],"working_directory":"core","timeout_ms":30000}),
          "eb0ff76a3f5dd4cc6f1e9966d03140ab49e08700fc0a179505f0ae46c1b74292")

if __name__ == "__main__":
    unittest.main()
