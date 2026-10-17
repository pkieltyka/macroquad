import assert from "node:assert/strict";
import test from "node:test";
import { witnesses, verifyPixels, comparePixels } from "./oracle.mjs";

function witnessImage() {
  const pixels = new Uint8Array(640 * 480 * 4);
  for (const [x, y, color] of witnesses) pixels.set(color, ((479 - y) * 640 + x) * 4);
  return pixels;
}
test("independent witnesses reject identical blank output and every corrupted landmark", () => {
  const pixels = witnessImage();
  assert.equal(verifyPixels(pixels, 640, 480).length, witnesses.length);
  assert.throws(() => verifyPixels(new Uint8Array(pixels.length), 640, 480), /tile border/);
  for (const [x, y, , name] of witnesses) {
    const broken = pixels.slice();
    broken[((479 - y) * 640 + x) * 4] ^= 1;
    assert.throws(() => verifyPixels(broken, 640, 480), { message: new RegExp(name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")) });
  }
  assert.throws(() => verifyPixels(pixels, 480, 640), /dimensions/);
});
test("equivalence includes pixels outside the independent witnesses", () => {
  const reference = witnessImage(), actual = reference.slice();
  comparePixels(reference, actual);
  actual[100] ^= 1;
  assert.throws(() => comparePixels(reference, actual), /byte 100/);
  assert.throws(() => comparePixels(reference, new Uint8Array(0)), /sizes differ/);
});
