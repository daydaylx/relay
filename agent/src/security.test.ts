import assert from "node:assert/strict";
import test from "node:test";
import { containsLikelySecret } from "./security.js";

test("likely credentials and private keys are blocked from every provider", () => {
  for (const text of [
    "api_key=sk-verylongcredentialvalue123456",
    "my password is hunter2verylong",
    "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.signature1234",
    "-----BEGIN OPENSSH PRIVATE KEY-----",
    "ghp_abcdefghijklmnopqrstuvwxyz1234567890",
  ]) assert.equal(containsLikelySecret(text), true, text);
});

test("ordinary diagnostic requests remain usable", () => {
  for (const text of ["Why is Bluetooth not working?", "Install VLC", "What services failed?"]) {
    assert.equal(containsLikelySecret(text), false, text);
  }
});
