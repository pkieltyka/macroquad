import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import vm from "node:vm";

const source = fs.readFileSync(new URL("audio_unlock_gate.js", import.meta.url), "utf8");
function fixture({ initiallyRunning = false, resumeRuns = true, emitsStateChange = true } = {}) {
  const listeners = [];
  const context = {
    state: initiallyRunning ? "running" : "suspended",
    addEventListener: (_name, listener) => listeners.push(listener),
    suspend() { this.state = "suspended"; return Promise.resolve(); },
    resume() {
      if (resumeRuns) {
        this.state = "running";
        if (emitsStateChange) for (const listener of listeners) listener();
      }
      return Promise.resolve();
    },
  };
  const evidence = { errors: [] };
  let now = 0;
  const scope = vm.createContext({ performance: { now: () => ++now } });
  vm.runInContext(source, scope);
  const gate = scope.createAudioUnlockGate(context, evidence, () => {});
  return { context, evidence, gate };
}
const settled = () => new Promise(setImmediate);

test("a real suspension precondition precedes trusted gesture, resume and running evidence", async () => {
  const { context, evidence, gate } = fixture({ initiallyRunning: true });
  await settled();
  assert.equal(context.state, "suspended");
  assert.equal(evidence.audioUnlock.ready, true);
  assert.equal(gate.complete(), false);
  gate.gesture({ isTrusted: true, type: "mousedown" });
  assert.equal(gate.complete(), false);
  await context.resume();
  await settled();
  const audio = evidence.audioUnlock;
  assert.ok(audio.gestureSequence < audio.resumeSequence && audio.resumeSequence < audio.runningSequence);
  assert.equal(gate.complete(), true);
  assert.deepEqual(evidence.errors, []);
});

test("a synthetic gesture or a running context without a later runtime resume cannot unlock", async () => {
  const { context, gate } = fixture();
  await settled();
  gate.gesture({ isTrusted: false, type: "mousedown" });
  await context.resume();
  await settled();
  assert.equal(gate.complete(), false);
});

test("a settled resume Promise without a running context is insufficient", async () => {
  const { context, evidence, gate } = fixture({ resumeRuns: false });
  await settled();
  gate.gesture({ isTrusted: true, type: "mousedown" });
  await context.resume();
  await settled();
  assert.equal(evidence.audioUnlock.state, "suspended");
  assert.equal(gate.complete(), false);
});

test("running reported at Promise settlement without a statechange event is insufficient", async () => {
  const { context, gate } = fixture({ emitsStateChange: false });
  await settled();
  gate.gesture({ isTrusted: true, type: "mousedown" });
  await context.resume();
  await settled();
  assert.equal(context.state, "running");
  assert.equal(gate.complete(), false);
});

test("a trusted gesture arriving before suspension is ready is rejected", () => {
  const { evidence, gate } = fixture();
  gate.gesture({ isTrusted: true, type: "mousedown" });
  assert.match(evidence.errors[0], /preceded the suspended audio precondition/);
  assert.equal(gate.complete(), false);
});
