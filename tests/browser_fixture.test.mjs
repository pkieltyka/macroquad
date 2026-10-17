import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import vm from "node:vm";

function fixture(name, search = "") {
  const callbacks = new Map(), events = [], listeners = new Map();
  let nextFrame = 0, clock = 0;
  const canvas = { addEventListener() {}, dispatchEvent(event) { events.push(event); this.onEvent?.(event); } };
  class GL {
    getParameter() { return this.binding; }
    bufferData() {} bufferSubData() {} deleteBuffer() {} createBuffer() { return {}; }
    ARRAY_BUFFER = 1; ARRAY_BUFFER_BINDING = 2; ELEMENT_ARRAY_BUFFER_BINDING = 3;
    binding = {};
  }
  class Canvas { getContext() {} }
  class SyntheticEvent {
    constructor(type, fields) { this.type = type; Object.assign(this, fields); this.isTrusted = false; }
  }
  const addEventListener = (name, callback) => listeners.set(name, [...(listeners.get(name) ?? []), callback]);
  const context = vm.createContext({
    URLSearchParams, queueMicrotask, location: { search }, performance: { now: () => clock },
    HTMLCanvasElement: Canvas, WebGLRenderingContext: GL, WebGL2RenderingContext: class extends GL {},
    console: Object.fromEntries(["log", "info", "warn", "error"].map(name => [name, () => {}])),
    Event: SyntheticEvent, MouseEvent: SyntheticEvent, KeyboardEvent: SyntheticEvent,
    document: { querySelector: () => canvas, getElementById: () => canvas,
      hasFocus: () => true, visibilityState: "visible", addEventListener },
    requestAnimationFrame(callback) { const id = ++nextFrame; callbacks.set(id, callback); return id; },
    cancelAnimationFrame(id) { callbacks.delete(id); }, addEventListener,
  });
  context.window = context;
  const html = fs.readFileSync(new URL(`./${name}/case.html`, import.meta.url), "utf8");
  vm.runInContext(html.match(/<script>([\s\S]*?)<\/script>/)[1], context);
  return { context, canvas, events, emit: (name, event) => { for (const callback of listeners.get(name) ?? []) callback(event); }, gl: new GL(), tick: async () => {
    clock += 16;
    const frame = [...callbacks]; callbacks.clear();
    for (const [, callback] of frame) callback(clock);
    await Promise.resolve();
  } };
}

test("capacity observer rejects unknown allocations and overflows, accepting a known in-bounds upload", () => {
  for (const [allocation, length, expected] of [[null, 4, "unknown"], [NaN, 4, "unknown"], [4, 8, "exceeds"], [4, 4, null]]) {
    const f = fixture("browser_capacity");
    if (allocation !== null) f.gl.bufferData(f.gl.ARRAY_BUFFER, allocation);
    f.gl.bufferSubData(f.gl.ARRAY_BUFFER, 0, new Uint8Array(length));
    if (expected) assert.ok(f.context.evidence.errors.some(message => message.includes(expected)));
    else assert.equal(f.context.evidence.errors.length, 0);
  }
});

test("fresh trusted DOM focus transitions remain observable before the deferred Rust Recorder flush", () => {
  const f = fixture("browser_input");
  f.context.document.hasFocus = () => false;
  f.emit("blur", {type: "blur", isTrusted: true});
  f.context.document.visibilityState = "hidden";
  f.emit("visibilitychange", {type: "visibilitychange", isTrusted: true});
  f.context.document.hasFocus = () => true;
  f.context.document.visibilityState = "visible";
  f.emit("focus", {type: "focus", isTrusted: true});
  assert.deepEqual(Array.from(f.context.evidence.domFocus, event => event.focused), [false, true]);
  assert.ok(f.context.evidence.domFocus.every(event => event.trusted));
  assert.equal(f.context.evidence.focus.length, 0, "DOM observation must not invent Rust callbacks");
});

test("positive fixtures count console errors and labelled old-v1 permits only its specific actual trap", () => {
  for (const name of ["browser_capacity", "browser_input"]) {
    const f = fixture(name);
    f.context.console.error("unexpected error");
    assert.equal(f.context.evidence.errors[0], "unexpected error");
  }
  const negative = fixture("browser_capacity", "?selectionVersion=1");
  negative.context.console.error(new Error("unrelated startup failure"));
  negative.context.console.error("RuntimeError: unreachable executed");
  assert.equal(negative.context.evidence.errors.length, 1);
  assert.equal(negative.context.evidence.expectedStartupErrors[0], "RuntimeError: unreachable executed");
  const positive = fixture("browser_capacity");
  positive.context.console.error("RuntimeError: unreachable");
  assert.equal(positive.context.evidence.errors[0], "RuntimeError: unreachable");
});

test("synthetic touch evidence carries consistent authoritative active-contact lists", () => {
  const f = fixture("browser_input");
  f.context.syntheticTouch("touchstart", [11, 12]);
  f.context.syntheticTouch("touchend", [11]);
  f.context.syntheticTouch("touchcancel", [12]);
  assert.deepEqual(f.events.map(event => Array.from(event.touches, touch => touch.identifier)), [[11, 12], [12], []]);
  assert.deepEqual(Array.from(f.context.evidence.synthetic, event => event.trusted), [false, false, false]);
});

test("a pending old frame cannot satisfy a fresh-event blocking wake assertion", async () => {
  for (const schedules of [false, true]) {
    const f = fixture("browser_input");
    f.context.requestAnimationFrame(() => f.context.evidence.frames.push({frame: 0}));
    f.canvas.onEvent = () => { if (schedules) f.context.requestAnimationFrame(() => f.context.evidence.frames.push({frame: 1})); };
    const result = f.context.inputWake("touchstart", [7]).then(value => ({value}), error => ({error}));
    for (let frame = 0; frame < 5; frame++) await f.tick();
    const outcome = await result;
    if (!schedules) assert.match(outcome.error.message, /Fresh event did not schedule/);
    else {
      assert.equal(outcome.value.before.pending, 0);
      assert.equal(outcome.value.before.frameCount, 1);
      assert.ok(outcome.value.after.requests > outcome.value.before.requests);
      assert.equal(f.context.evidence.frames.length, 2);
    }
  }
});


test("native-menu audit records late default prevention without suppressing the menu", async () => {
  const f = fixture("browser_input", "?nativeMenu=1");
  const event = {type:"contextmenu",isTrusted:true,button:2,defaultPrevented:false};
  f.emit("contextmenu",event);
  event.defaultPrevented = true;
  await Promise.resolve();
  assert.equal(f.context.evidence.nativeAudit[0].trusted,true);
  assert.equal(f.context.evidence.nativeAudit[0].defaultPrevented,true);
});

test("labelled lost-terminal fixture can provide a later authoritative touches list", () => {
  const f = fixture("browser_input");
  f.context.syntheticTouch("touchstart",[71]);
  f.context.syntheticTouch("touchstart",[72],[72]);
  f.context.syntheticTouch("touchcancel",[72],[]);
  assert.deepEqual(f.events.map(event=>Array.from(event.touches,touch=>touch.identifier)),[[71],[72],[]]);
  assert.deepEqual(f.events.map(event=>Array.from(event.changedTouches,touch=>touch.identifier)),[[71],[72],[72]]);
});
