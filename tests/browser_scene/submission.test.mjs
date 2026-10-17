import test from "node:test";
import assert from "node:assert/strict";
import { observeSubmission } from "./submission.mjs";

function context() {
    const calls = [];
    const prototype = { ARRAY_BUFFER:34962, ELEMENT_ARRAY_BUFFER:34963 };
    for (const name of ["bufferData", "bufferSubData", "bindFramebuffer", "drawElements", "drawArrays", "drawElementsInstanced", "drawArraysInstanced",
        "createBuffer", "deleteBuffer", "createTexture", "deleteTexture", "bindTexture", "texImage2D", "texSubImage2D"]) {
        prototype[name] = function (...args) {
            assert.equal(this, gl);
            calls.push({ name, args });
            return sentinel;
        };
    }
    const gl = Object.create(prototype), sentinel = {};
    return { gl, calls, sentinel };
}

test("observes returned calls, bytes and work, preserving receiver and results; restores inherited methods", () => {
    const { gl, calls, sentinel } = context();
    const original = gl.bufferSubData;
    const finish = observeSubmission(gl);
    assert.equal(gl.bufferSubData(gl.ARRAY_BUFFER, 0, new Uint8Array(320)), sentinel);
    gl.bufferSubData(gl.ELEMENT_ARRAY_BUFFER, 0, new Uint8Array(24));
    gl.bufferData(gl.ARRAY_BUFFER, 1024, 0);
    gl.bufferData(gl.ARRAY_BUFFER, new Uint8Array(8), 0);
    gl.drawElements(4, 12, 5123, 0);
    gl.drawArrays(4, 0, 3);
    gl.drawElementsInstanced(4, 6, 5123, 0, 2);
    gl.drawArraysInstanced(4, 0, 4, 3);
    gl.bindFramebuffer(36160, null);
    gl.texSubImage2D(3553, 0, 0, 0, 1, 1, 6408, 5121, new Uint8Array(4));
    const counts = finish();
    assert.equal(counts.vertexUploadBytes, 320);
    assert.equal(counts.indexUploadBytes, 24);
    assert.equal(counts.bufferDataBytes, 1032);
    assert.equal(counts.submittedIndices, 24);
    assert.equal(counts.submittedVertices, 15);
    assert.equal(counts.calls.bufferSubData, 2);
    assert.equal(counts.calls.texSubImage2D, 1);
    assert.equal(calls.length, 10);
    assert.equal(gl.bufferSubData, original);
    assert.equal(Object.hasOwn(gl, "bufferSubData"), false);
});

test("native exceptions propagate unchanged and own properties restore exactly", () => {
    const { gl } = context(), error = new Error("native failure");
    Object.defineProperty(gl, "drawElements", { value: function () { throw error; }, writable:true, configurable:true });
    const descriptor = Object.getOwnPropertyDescriptor(gl, "drawElements");
    const finish = observeSubmission(gl);
    assert.throws(() => gl.drawElements(4, 12, 5123, 0), value => value === error);
    assert.equal(finish().calls.drawElements, undefined);
    assert.deepEqual(Object.getOwnPropertyDescriptor(gl, "drawElements"), descriptor);
});

test("unsupported upload overload fails explicitly rather than reporting invented bytes", () => {
    const { gl } = context();
    const finish = observeSubmission(gl);
    assert.throws(() => gl.bufferSubData(gl.ARRAY_BUFFER, 0, new Uint8Array(4), 0, 2), /Unsupported/);
    finish();
});
