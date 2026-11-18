import { canonicalActionDigest } from "./canonicalActionDigest";

describe("Canonical action digest v1 vectors", () => {
  test("read_file", async () => {
    expect(await canonicalActionDigest({ type: "read_file", path: "src/main.rs", max_bytes: 1024 }))
      .toBe("87cc9be51307092dd5c6d9203de172422cabdc506f78bcb224b07f39d15022a7");
  });
  test("propose_patch", async () => {
    expect(await canonicalActionDigest({ type: "propose_patch", path: "src/lib.rs", patch: "diff --git a b", expected_target_sha256: "a".repeat(64) }))
      .toBe("d8ae37db445a5bf6b34239e8032fa0abb239e130c026ace019ecdaaf232de192");
  });
  test("run_command", async () => {
    expect(await canonicalActionDigest({ type: "run_command", executable: "cargo", args: ["test", "--lib"], working_directory: "core", timeout_ms: 30000 }))
      .toBe("eb0ff76a3f5dd4cc6f1e9966d03140ab49e08700fc0a179505f0ae46c1b74292");
  });
});
