// These fixed interior witnesses are independent of the Rust atlas/scene code.
export const witnesses = [
  [0, 0, [8, 12, 20, 255], "tile border"],
  [4, 4, [24, 40, 64, 255], "first tile"],
  [20, 4, [32, 64, 48, 255], "adjacent atlas cell"],
  [4, 420, [64, 40, 32, 255], "lower tile"],
  [636, 428, [32, 64, 48, 255], "last visible tile row"],
  [90, 90, [64, 208, 240, 255], "first ship"],
  [554, 218, [208, 96, 224, 255], "last ship"],
  [9, 251, [240, 80, 64, 255], "first projectile"],
  [598, 371, [240, 80, 64, 255], "last projectile"],
  [26, 33, [240, 224, 96, 255], "bitmap glyph"],
  [24, 34, [48, 32, 64, 255], "transparent glyph margin"],
  [82, 81, [240, 224, 96, 255], "label over ship"],
  [630, 460, [8, 16, 32, 255], "HUD over tiles"],
  [18, 449, [240, 224, 96, 255], "HUD glyph"],
];

export function verifyPixels(pixels, width, height) {
  if (width !== 640 || height !== 480 || pixels.length !== width * height * 4) throw new Error("Unexpected framebuffer dimensions");
  return witnesses.map(([x, y, expected, name]) => {
    // WebGL readPixels uses a bottom-left origin; scene coordinates use top-left.
    const offset = ((height - 1 - y) * width + x) * 4;
    const actual = Array.from(pixels.slice(offset, offset + 4));
    if (actual.some((value, i) => value !== expected[i])) throw new Error(`${name} (${x},${y}): ${actual}, expected ${expected}`);
    return { name, x, y, rgba: actual };
  });
}

export function comparePixels(reference, actual) {
  if (reference.length !== actual.length) throw new Error("Framebuffer sizes differ");
  const index = reference.findIndex((value, i) => value !== actual[i]);
  if (index !== -1) throw new Error(`Variants differ at RGBA byte ${index}: ${reference[index]} vs ${actual[index]}`);
}
