// Installed only for the post-timing diagnostic frame; never wraps timing trials.
export function observeSubmission(gl) {
    const counts = { calls: {}, vertexUploadBytes: 0, indexUploadBytes: 0,
        bufferDataBytes: 0, submittedIndices: 0, submittedVertices: 0 };
    const originals = new Map();
    const methods = ["bufferData", "bufferSubData", "bindFramebuffer", "drawElements", "drawArrays", "drawElementsInstanced", "drawArraysInstanced",
        "createBuffer", "deleteBuffer", "createTexture", "deleteTexture", "bindTexture",
        "texImage2D", "texSubImage2D"];
    for (const name of methods) {
        const descriptor = Object.getOwnPropertyDescriptor(gl, name);
        const original = gl[name];
        originals.set(name, { descriptor, original });
        gl[name] = function (...args) {
            // Native receiver, result and exceptions are preserved. Count returned calls,
            // including GL errors (the separate pixel gate still rejects those errors).
            const result = original.apply(this, args);
            counts.calls[name] = (counts.calls[name] || 0) + 1;
            if (name === "bufferSubData") {
                // The paired runtime supplies Uint8Array without WebGL2 range args.
                if (args.length !== 3 || !ArrayBuffer.isView(args[2])) throw new Error("Unsupported bufferSubData overload");
                if (args[0] === gl.ARRAY_BUFFER) counts.vertexUploadBytes += args[2].byteLength;
                else if (args[0] === gl.ELEMENT_ARRAY_BUFFER) counts.indexUploadBytes += args[2].byteLength;
                else throw new Error("Unexpected buffer upload target");
            }
            if (name === "bufferData") counts.bufferDataBytes += typeof args[1] === "number" ? args[1] : args[1].byteLength;
            if (name === "drawElements") counts.submittedIndices += args[1];
            if (name === "drawArrays") counts.submittedVertices += args[2];
            if (name === "drawElementsInstanced") counts.submittedIndices += args[1] * args[4];
            if (name === "drawArraysInstanced") counts.submittedVertices += args[2] * args[3];
            return result;
        };
    }
    return () => {
        for (const [name, { descriptor }] of originals) {
            if (descriptor) Object.defineProperty(gl, name, descriptor);
            else delete gl[name];
        }
        return counts;
    };
}
