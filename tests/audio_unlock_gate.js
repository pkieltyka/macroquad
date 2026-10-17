// Observe a real runtime context; the fixture never resumes it on the runtime's behalf.
function createAudioUnlockGate(context, evidence, changed) {
    let sequence = 0;
    const audio = evidence.audioUnlock = {
        ready: false, initialState: context.state, state: context.state,
        trustedGesture: false, suspendedBeforeGesture: false,
        gestureSequence: 0, resumeSequence: 0, runningSequence: 0,
        runningAfterTrustedGesture: false, states: [],
    };
    const observe = type => {
        audio.state = context.state;
        audio.states.push({ sequence: ++sequence, type, state: context.state, time: performance.now() });
        if (type === "statechange" && context.state === "running" && audio.gestureSequence > 0 && audio.resumeSequence > audio.gestureSequence) {
            audio.runningSequence = sequence;
            audio.runningAfterTrustedGesture = true;
        }
        changed();
    };
    context.addEventListener("statechange", () => observe("statechange"));
    const resume = context.resume.bind(context);
    context.resume = (...args) => {
        audio.resumeSequence = ++sequence;
        const result = resume(...args);
        Promise.resolve(result).then(() => observe("resume-settled"), error => {
            evidence.errors.push(`Runtime audio resume failed: ${error}`);
        });
        return result;
    };
    // Some browsers initially allow audio. Establish a real suspended context
    // so a permissive autoplay policy cannot masquerade as a successful unlock.
    Promise.resolve(context.suspend()).then(() => {
        audio.ready = context.state === "suspended";
        if (!audio.ready) evidence.errors.push("Audio unlock precondition is not suspended");
        observe("suspended-ready");
    }, error => evidence.errors.push(`Could not suspend runtime audio context: ${error}`));
    return {
        gesture(event) {
            if (!event.isTrusted || audio.trustedGesture) return;
            audio.trustedGesture = true;
            audio.suspendedBeforeGesture = audio.ready && context.state === "suspended";
            audio.gestureType = event.type;
            audio.gestureSequence = ++sequence;
            if (!audio.suspendedBeforeGesture) evidence.errors.push("Trusted gesture preceded the suspended audio precondition");
            changed();
        },
        complete() {
            return audio.suspendedBeforeGesture && audio.runningAfterTrustedGesture && context.state === "running";
        },
    };
}
