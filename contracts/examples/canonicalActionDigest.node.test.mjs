import test from "node:test";
import assert from "node:assert/strict";
import { canonicalActionDigest } from "./canonicalActionDigest.ts";

test("canonical action digest v1 shared vectors", async () => {
  assert.equal(await canonicalActionDigest({type:"read_file",path:"src/main.rs",max_bytes:1024}),
    "87cc9be51307092dd5c6d9203de172422cabdc506f78bcb224b07f39d15022a7");
  assert.equal(await canonicalActionDigest({type:"propose_patch",path:"src/lib.rs",patch:"diff --git a b",expected_target_sha256:"a".repeat(64)},
    "d8ae37db445a5bf6b34239e8032fa0abb239e130c026ace019ecdaaf232de192");
  assert.equal(await canonicalActionDigest({type:"run_command",executable:"cargo",args:["test","--lib"],working_directory:"core",timeout_ms:30000}),
    "eb0ff76a3f5dd4cc6f1e9966d03140ab49e08700fc0a179505f0ae46c1b74292");
});
