# Supported backend ownership and X11 buffers

Reviewed against published Miniquad 0.4.11 on 2026-10-16. These are narrow local
dependency corrections; the exact archive, attribution and complete reproducible
delta are documented in [vendor/README.md](../vendor/README.md). They do not
establish closure of the fork's remaining context/security work.

## Render passes borrow their attachments

Metal releases only its render-pass descriptor. OpenGL previously deleted the
pass's color/depth/resolve textures as well as its framebuffer objects, despite
the caller still owning those texture IDs. A retained render-target texture
could become black/invalid as soon as its pass was deleted; a later explicit
texture deletion could also target an already-deleted or reused GL name.

The local GL implementation now deletes only the primary and resolve FBOs.
Callers must retain attachment owners until passes no longer use them, then
delete textures explicitly when their last owner drops. This intentionally
changes OpenGL's deletion behavior to match Metal. Macroquad's ownership
integration must retain color, depth and resolve guards and queue pass cleanup
before last-owner texture cleanup; direct backend consumers must follow the
same ordering. Backend deletion is not a replacement for ownership checks.

The direct fixture in `tests/texture_cache/regression.rs` creates a pass, clears
it, deletes it, samples the retained output, and recreates a pass over the same
attachments. It checks framebuffer completeness, exact pixels and GL errors,
with/without depth and four-sample resolves where supported. Owners explicitly
delete attachments afterward. The original GL behavior reproduces black pixels
after the first pass deletion. Existing texture-cache churn checks remain in
the same fixture.

That coverage also exposed a capability mismatch: WebGL1 version strings include
a vendor suffix, so equality with `WebGL 1.0` could wrongly advertise MSAA
resolves. Prefix detection now identifies WebGL1; the fixture asserts the
advertised capability and runs non-MSAA there, with MSAA additionally in WebGL2.
WebGL2 also rejected the previous unsized standard depth allocation with
INVALID_VALUE, leaving an incomplete framebuffer. Creation and resize now use
DEPTH_COMPONENT16 for `TextureFormat::Depth` on WebGL2, keeping the existing
UNSIGNED_SHORT pixel type and native/WebGL1 combinations. The fixture resizes
and reattaches that depth texture too. This follows the
[WebGL2 texture-format rules](https://registry.khronos.org/webgl/specs/2.0.0/);
it is not a general depth/float-format redesign.
Format selection reuses the cached owned version string and introduces no GL
query. The existing initial GL_VERSION copy now checks null before constructing
a C string and copies non-UTF-8 vendor text lossily. Empty/no-context metadata
advertises no resolves. CPU/Miri tests cover null copying and cached WebGL1
vendor suffixes/WebGL2 capability behavior; this does not implement restoration
of a lost graphics context.
Native GL coverage uses Xvfb/software rendering, not a physical GPU matrix.

## Xlib owns property buffers until XFree

The [Xlib property manual](https://raw.githubusercontent.com/mirror/libX11/master/man/XGetWindowProperty.man)
specifies native char, short and long arrays for formats 8, 16 and 32. Format 32
expands to eight-byte longs on LP64. The previous format-16 path multiplied by
`sizeof(int)` rather than `sizeof(short)`, allowing an overread. An INCR header
and intermediate chunks also escaped XFree.

Each reply now owns its Xlib allocation through a scoped guard. Before forming
a slice, the actual helper checks success, nonzero type, supported format,
complete-property status, checked item/byte multiplication, nonnull data for
positive lengths, and the Rust slice/accumulated-vector length limit. Empty
properties do not construct a raw slice from a null pointer. Invalid replies
abort without returning partial data. Format-32 output retains native-long
layout for existing atom-list consumers.

The [ICCCM INCR protocol](https://www.x.org/releases/X11R7.7/doc/xorg-docs/icccm/icccm.html#INCR_Properties)
uses an initial one-item format-32 header, consistent typed chunks and a final
zero-length property. All allocations are released before acknowledgement or
another Xlib call, including error/header/terminator paths. Notifications must
match the requested window and property. The receiver acknowledges the final
empty chunk too. Existing blocking behavior remains: a selection owner that
never responds can still stall the operation; this is not a timeout redesign.

Mutable global message storage is replaced by per-thread immutable `Rc<str>`
snapshots. Allocations/replacements and the outgoing signed item-count check
happen outside FFI; no TLS borrow survives an Xlib call. A snapshot stays valid
if callbacks replace the current message.

`property_tests` exercises actual production helpers with exact-sized short and
native-long allocations, invalid status/type/format/count/null metadata,
aggregate overflow, per-chunk deallocation, INCR validation, and thread-local
replacement. The CPU cases run under Miri on x86_64 and i686. The explicitly run
live Xvfb test checks real format-16/32 properties and UTF-8/empty clipboard
self-selection round trips. Live multi-client INCR timing and other native
platforms are outside this finite test; ownership/freeing of INCR replies is
covered by the shared-helper tests.
