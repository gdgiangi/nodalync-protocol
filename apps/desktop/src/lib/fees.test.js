import test from "node:test";
import assert from "node:assert/strict";
import { hbarToTinybars, formatHbar, displayTransactionTime } from "./fees.js";

test("HBAR quotes preserve one tinybar and accept a free query", () => {
  assert.equal(hbarToTinybars("0.00000001"), 1);
  assert.equal(hbarToTinybars("0.01"), 1_000_000);
  assert.equal(hbarToTinybars("1.23456789"), 123_456_789);
  assert.equal(hbarToTinybars("0"), 0);
});

test("quotes reject negative, partial, overly precise, and unsafe amounts", () => {
  for (const value of ["", "-1", "1e8", "0.123456789", "1x", "99999999999999999"]) {
    assert.throws(() => hbarToTinybars(value));
  }
});

test("transaction amounts display the backend tinybar unit as HBAR", () => {
  assert.equal(formatHbar(1), "0.00000001 HBAR");
  assert.equal(formatHbar(100_000_000), "1 HBAR");
  assert.equal(formatHbar(0), "0 HBAR");
  assert.equal(formatHbar(null), "—");
});

test("transaction dates use ISO created_at and tolerate missing values", () => {
  assert.equal(displayTransactionTime("not a date"), "—");
  assert.equal(displayTransactionTime(null), "—");
  assert.notEqual(displayTransactionTime("2026-09-07T22:00:00Z"), "—");
});
