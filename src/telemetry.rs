//! Thread-local frame profiling and owned capture snapshots.
use std::{cell::RefCell, collections::HashSet, marker::PhantomData, rc::Rc};

thread_local! {
    static PROFILER: RefCell<Profiler> = RefCell::new(Profiler::new());
}

fn with_profiler<R>(f: impl FnOnce(&mut Profiler) -> R) -> R {
    PROFILER.with(|profiler| f(&mut profiler.borrow_mut()))
}

// Event/render zones may run while Context is already borrowed. Reading its
// start_time through get_time() would recursively borrow that Context.
fn clock() -> f64 {
    miniquad::date::now()
}

#[derive(Debug, Clone)]
pub struct Zone {
    pub name: String,
    /// Seconds on Miniquad's context-independent clock.
    pub start_time: f64,
    pub duration: f64,
    pub children: Vec<Zone>,
}

/// A zone belonging to the thread and frame in which it was started.
///
/// Dropping an ancestor also closes its still-open descendants. Later drops of
/// those descendants, or guards retained across a frame boundary, are harmless.
///
/// ```compile_fail
/// let guard = macroquad::telemetry::ZoneGuard::new("thread-affine");
/// std::thread::spawn(move || drop(guard));
/// ```
pub struct ZoneGuard {
    token: Option<u64>,
    _thread: PhantomData<Rc<()>>,
}

impl ZoneGuard {
    pub fn new(name: &str) -> ZoneGuard {
        ZoneGuard {
            token: start_zone(name),
            _thread: PhantomData,
        }
    }
}

impl Drop for ZoneGuard {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            let now = clock();
            // A guard may drop during unwinding or after the profiler's TLS
            // destructor. Never cause a second panic; reset closes any zone
            // whose guard could not obtain this short borrow.
            let _ = PROFILER.try_with(|profiler| {
                if let Ok(mut profiler) = profiler.try_borrow_mut() {
                    profiler.finish_token(token, now);
                }
            });
        }
    }
}

pub fn enable() {
    with_profiler(|profiler| profiler.enable_request = Some(true));
}

pub fn disable() {
    with_profiler(|profiler| profiler.enable_request = Some(false));
}

pub fn is_enabled() -> bool {
    with_profiler(|profiler| profiler.enabled)
}

fn start_zone(name: &str) -> Option<u64> {
    if !is_enabled() {
        return None;
    }
    let now = clock();
    with_profiler(|profiler| profiler.begin_zone(name, now))
}

pub fn begin_zone(name: &str) {
    start_zone(name);
}

pub fn end_zone() {
    let now = clock();
    let paired = with_profiler(|profiler| !profiler.enabled || profiler.finish_last(now));
    // Validate outside the borrow: a panic hook can itself inspect telemetry.
    assert!(paired, "end_zone called without begin_zone");
}

/// Miniquad 0.4.11 does not implement elapsed-query result retrieval or a safe
/// capability check. GPU timings are unavailable on every supported backend.
pub fn gpu_timing_supported() -> bool {
    false
}

/// Records a query label and checks pairing without issuing a GPU operation.
/// See [`gpu_timing_supported`].
pub fn begin_gpu_query(name: &str) {
    let paired = with_profiler(|profiler| profiler.begin_gpu_label(name));
    assert!(paired, "Only one active GPU query label is allowed");
}

pub fn end_gpu_query() {
    let paired = with_profiler(|profiler| profiler.active_query.take().is_some());
    assert!(paired, "End GPU query without begin GPU query");
}

/// Returns recorded labels separately from the unavailable timing samples.
pub fn gpu_query_labels() -> Vec<String> {
    with_profiler(|profiler| {
        let mut labels: Vec<_> = profiler.query_labels.iter().cloned().collect();
        labels.sort();
        labels
    })
}

/// Returns no samples while GPU timing is unavailable, rather than reporting
/// unavailable results as measured zero durations.
pub fn gpu_queries() -> Vec<(String, u64)> {
    vec![]
}

/// Has no effect while [`gpu_timing_supported`] is false.
pub fn sample_gpu_queries() {}

/// Temporarily excludes debug rendering from an active capture. Calls nest.
#[doc(hidden)]
pub fn pause_gl_capture() {
    let changed = with_profiler(|profiler| profiler.pause_capture());
    if changed {
        crate::with_context(|context| context.gl.capture(false));
    }
}

pub fn resume_gl_capture() {
    let changed = with_profiler(|profiler| profiler.resume_capture());
    if changed {
        crate::with_context(|context| context.gl.capture(true));
    }
}

pub(crate) fn reset() {
    let frame_time = crate::time::get_frame_time();
    let now = clock();
    let effects = with_profiler(|profiler| profiler.advance_frame(frame_time, now));
    drop(effects.released_drawcalls);
    if let Some(capture) = effects.capture {
        crate::with_context(|context| context.gl.capture(capture));
    }
}

// Called before the graphics owner is deactivated. Public snapshot clones can
// outlive the engine; their eventual drops only queue on their original owner.
pub(crate) fn shutdown() {
    let drawcalls = with_profiler(|profiler| {
        profiler.capture = false;
        profiler.capture_request = false;
        profiler.capture_pauses = 0;
        profiler.active_query = None;
        std::mem::take(&mut profiler.drawcalls)
    });
    drop(drawcalls);
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub full_frame_time: f32,
    pub zones: Vec<Zone>,
    active_zones: usize,
}

impl Frame {
    const fn new() -> Frame {
        Frame {
            full_frame_time: 0.0,
            zones: vec![],
            active_zones: 0,
        }
    }

    pub fn try_clone(&self) -> Option<Frame> {
        (self.active_zones == 0).then(|| self.clone())
    }
}

pub fn frame() -> Frame {
    with_profiler(|profiler| profiler.prev_frame.clone())
}

struct ActiveZone {
    token: u64,
    path: Vec<usize>,
}

struct ResetEffects {
    capture: Option<bool>,
    released_drawcalls: Vec<DrawCallTelemetry>,
}

struct Profiler {
    frame: Frame,
    prev_frame: Frame,
    active_zones: Vec<ActiveZone>,
    next_zone_token: u64,
    query_labels: HashSet<String>,
    active_query: Option<String>,
    enabled: bool,
    capture_request: bool,
    capture: bool,
    capture_pauses: usize,
    enable_request: Option<bool>,
    drawcalls: Vec<DrawCallTelemetry>,
    strings: Vec<String>,
}

impl Profiler {
    fn new() -> Self {
        Self {
            frame: Frame::new(),
            prev_frame: Frame::new(),
            active_zones: vec![],
            next_zone_token: 0,
            query_labels: HashSet::new(),
            active_query: None,
            enabled: false,
            capture_request: false,
            capture: false,
            capture_pauses: 0,
            enable_request: None,
            drawcalls: vec![],
            strings: vec![],
        }
    }

    fn begin_gpu_label(&mut self, name: &str) -> bool {
        if self.active_query.is_some() {
            return false;
        }
        self.query_labels.insert(name.to_owned());
        self.active_query = Some(name.to_owned());
        true
    }

    fn children_at<'a>(mut children: &'a mut Vec<Zone>, path: &[usize]) -> &'a mut Vec<Zone> {
        for &index in path {
            children = &mut children[index].children;
        }
        children
    }

    fn begin_zone(&mut self, name: &str, now: f64) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        // Never reuse a token held by a guard retained across frame boundaries.
        let next = self.next_zone_token.checked_add(1)?;
        let token = self.next_zone_token;
        self.next_zone_token = next;
        let mut path = self
            .active_zones
            .last()
            .map_or_else(Vec::new, |zone| zone.path.clone());
        let children = Self::children_at(&mut self.frame.zones, &path);
        let index = children.len();
        children.push(Zone {
            name: name.to_owned(),
            start_time: now,
            duration: 0.0,
            children: vec![],
        });
        path.push(index);
        self.active_zones.push(ActiveZone { token, path });
        self.frame.active_zones = self.active_zones.len();
        Some(token)
    }

    fn finish_from(&mut self, position: usize, now: f64) {
        while self.active_zones.len() > position {
            let active = self.active_zones.pop().unwrap();
            let (index, parents) = active.path.split_last().unwrap();
            let zone = &mut Self::children_at(&mut self.frame.zones, parents)[*index];
            zone.duration = (now - zone.start_time).max(0.0);
        }
        self.frame.active_zones = self.active_zones.len();
    }

    fn finish_last(&mut self, now: f64) -> bool {
        if self.active_zones.is_empty() {
            return false;
        }
        self.finish_from(self.active_zones.len() - 1, now);
        true
    }

    fn finish_token(&mut self, token: u64, now: f64) {
        if let Some(position) = self
            .active_zones
            .iter()
            .position(|zone| zone.token == token)
        {
            self.finish_from(position, now);
        }
    }

    fn pause_capture(&mut self) -> bool {
        if !self.capture {
            return false;
        }
        let was_paused = self.capture_pauses != 0;
        self.capture_pauses = self.capture_pauses.saturating_add(1);
        !was_paused
    }

    fn resume_capture(&mut self) -> bool {
        if self.capture_pauses == 0 {
            return false;
        }
        self.capture_pauses -= 1;
        self.capture && self.capture_pauses == 0
    }

    fn advance_frame(&mut self, frame_time: f32, now: f64) -> ResetEffects {
        self.finish_from(0, now);
        self.frame.full_frame_time = frame_time;
        self.prev_frame = std::mem::replace(&mut self.frame, Frame::new());
        if let Some(enable) = self.enable_request.take() {
            self.enabled = enable;
        }
        let capture = std::mem::take(&mut self.capture_request);
        let changed = self.capture != capture || (self.capture && self.capture_pauses != 0);
        self.capture = capture;
        self.capture_pauses = 0;
        ResetEffects {
            capture: (changed || capture).then_some(capture),
            released_drawcalls: if capture {
                std::mem::take(&mut self.drawcalls)
            } else {
                vec![]
            },
        }
    }
}

/// Records elapsed wall-clock time in [`strings`].
pub struct LogTimeGuard<'a> {
    name: &'a str,
    start_time: f64,
}

impl<'a> LogTimeGuard<'a> {
    pub fn new(name: &'a str) -> LogTimeGuard<'a> {
        LogTimeGuard {
            name,
            start_time: clock(),
        }
    }
}

impl Drop for LogTimeGuard<'_> {
    fn drop(&mut self) {
        let message = format!(
            "Time query: {}, {:.1}s",
            self.name,
            clock() - self.start_time
        );
        let _ = PROFILER.try_with(|profiler| {
            if let Ok(mut profiler) = profiler.try_borrow_mut() {
                profiler.strings.push(message);
            }
        });
    }
}

pub fn log_string(string: &str) {
    with_profiler(|profiler| profiler.strings.push(string.to_owned()));
}

pub fn drawcalls() -> Vec<DrawCallTelemetry> {
    with_profiler(|profiler| profiler.drawcalls.clone())
}

pub fn strings() -> Vec<String> {
    with_profiler(|profiler| profiler.strings.clone())
}

/// Estimated scheduler entry storage, excluding boxed futures and result payloads.
pub fn coroutines_allocated_memory() -> usize {
    crate::experimental::coroutines::allocated_memory()
}

pub fn active_coroutines_count() -> usize {
    crate::experimental::coroutines::active_coroutines_count()
}

pub fn capture_frame() {
    with_profiler(|profiler| profiler.capture_request = true);
}

struct CaptureResources {
    owner: Rc<crate::GraphicsOwner>,
    texture: miniquad::TextureId,
    pass: Option<miniquad::RenderPass>,
}

impl Drop for CaptureResources {
    fn drop(&mut self) {
        if let Some(pass) = self.pass {
            self.owner.defer(crate::ResourceDelete::RenderPass(pass));
        }
        // Backend render passes are non-owning attachment containers.
        self.owner
            .defer(crate::ResourceDelete::Texture(self.texture));
    }
}

#[derive(Clone)]
pub struct DrawCallTelemetry {
    pub indices_count: usize,
    pub(crate) texture: miniquad::TextureId,
    _resources: Rc<CaptureResources>,
}

impl std::fmt::Debug for DrawCallTelemetry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DrawCallTelemetry")
            .field("indices_count", &self.indices_count)
            .field("texture", &self.texture)
            .finish()
    }
}

pub(crate) fn track_drawcall(
    backend: &mut dyn miniquad::RenderingBackend,
    pipeline: &miniquad::Pipeline,
    bindings: &miniquad::Bindings,
    indices_count: usize,
) {
    let owner = crate::graphics_owner();
    let texture = backend.new_render_texture(miniquad::TextureParams {
        width: 128,
        height: 128,
        ..Default::default()
    });
    let mut resources = CaptureResources {
        owner,
        texture,
        pass: None,
    };
    let pass = backend.new_render_pass(texture, None);
    resources.pass = Some(pass);
    backend.begin_pass(
        Some(pass),
        miniquad::PassAction::clear_color(0.4, 0.8, 0.4, 1.),
    );
    backend.apply_pipeline(pipeline);
    backend.apply_bindings(bindings);
    backend.draw(0, indices_count as _, 1);
    backend.end_render_pass();
    let drawcall = DrawCallTelemetry {
        indices_count,
        texture,
        _resources: Rc::new(resources),
    };
    with_profiler(|profiler| profiler.drawcalls.push(drawcall));
}

pub fn textures_count() -> usize {
    crate::with_context(|context| context.textures.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recording() -> Profiler {
        let mut profiler = Profiler::new();
        profiler.enabled = true;
        profiler
    }

    fn fresh_tls() {
        shutdown();
        with_profiler(|profiler| *profiler = recording());
    }

    #[test]
    fn tree_and_owned_snapshots_survive_reallocation_and_thread_transfer() {
        let mut profiler = recording();
        for index in 0..128 {
            let token = profiler.begin_zone(&format!("parent {index}"), 1.).unwrap();
            profiler.begin_zone("child", 2.);
            assert!(profiler.frame.try_clone().is_none());
            assert!(profiler.finish_last(3.));
            profiler.finish_token(token, 4.);
        }
        profiler.advance_frame(0.25, 5.);
        let mut snapshot = profiler.prev_frame.try_clone().unwrap();
        assert_eq!(snapshot.full_frame_time, 0.25);
        for zone in &snapshot.zones {
            assert_eq!(zone.duration, 3.);
            assert_eq!(zone.children[0].duration, 1.);
        }
        snapshot.zones[0].name = "changed clone".into();
        assert_eq!(profiler.prev_frame.zones[0].name, "parent 0");
        profiler.advance_frame(0.5, 6.);
        assert!(profiler.prev_frame.zones.is_empty());
        assert_eq!(
            std::thread::spawn(move || snapshot.zones.len())
                .join()
                .unwrap(),
            128
        );
    }

    #[test]
    fn out_of_order_guard_does_not_close_a_later_zone() {
        fresh_tls();
        let parent = ZoneGuard::new("parent");
        let child = ZoneGuard::new("child");
        drop(parent);
        assert_eq!(with_profiler(|p| p.active_zones.len()), 0);
        let later = ZoneGuard::new("later");
        drop(child);
        assert_eq!(with_profiler(|p| p.active_zones.len()), 1);
        drop(later);
        assert_eq!(with_profiler(|p| p.active_zones.len()), 0);
        assert_eq!(with_profiler(|p| p.frame.zones.len()), 2);
    }

    #[test]
    fn disabled_and_previous_frame_guards_cannot_end_new_zones() {
        fresh_tls();
        with_profiler(|p| p.enabled = false);
        let disabled = ZoneGuard::new("disabled");
        with_profiler(|p| p.enabled = true);
        let old = ZoneGuard::new("old frame");
        drop(disabled);
        assert_eq!(with_profiler(|p| p.active_zones.len()), 1);
        with_profiler(|p| {
            p.advance_frame(0.1, clock());
        });
        assert_eq!(frame().zones[0].name, "old frame");
        let current = ZoneGuard::new("current frame");
        drop(old);
        assert_eq!(with_profiler(|p| p.active_zones.len()), 1);
        drop(current);
    }

    #[test]
    fn unwinding_and_unavailable_profiler_borrow_leave_recoverable_state() {
        fresh_tls();
        let panic = std::panic::catch_unwind(|| {
            let _parent = ZoneGuard::new("parent");
            let _child = ZoneGuard::new("child");
            panic!("expected unwind");
        });
        assert!(panic.is_err());
        assert_eq!(with_profiler(|p| p.active_zones.len()), 0);
        let guard = ZoneGuard::new("borrowed on drop");
        with_profiler(|p| {
            drop(guard);
            assert_eq!(p.active_zones.len(), 1);
            p.advance_frame(0.1, clock());
            assert_eq!(p.active_zones.len(), 0);
        });
        begin_zone("balanced");
        end_zone();
        assert!(std::panic::catch_unwind(end_zone).is_err());
        begin_zone("after rejection");
        end_zone();
    }

    #[test]
    fn guard_can_drop_after_profiler_thread_local_destruction() {
        thread_local! {
            static LAST_GUARD: RefCell<Option<ZoneGuard>> = const { RefCell::new(None) };
        }
        std::thread::spawn(|| {
            // TLS destructors run in reverse initialization order.
            LAST_GUARD.with(|slot| {
                fresh_tls();
                *slot.borrow_mut() = Some(ZoneGuard::new("late drop"));
            });
        })
        .join()
        .unwrap();
    }

    #[test]
    fn profiler_state_is_thread_local() {
        fresh_tls();
        log_string("main");
        let child = std::thread::spawn(|| {
            assert!(!is_enabled());
            assert!(strings().is_empty());
            log_string("child");
            strings()
        })
        .join()
        .unwrap();
        assert_eq!(child, ["child"]);
        assert_eq!(strings(), ["main"]);
    }

    #[test]
    fn unavailable_gpu_timing_is_distinct_from_measured_zero() {
        fresh_tls();
        assert!(!gpu_timing_supported());
        begin_gpu_query("work");
        assert!(std::panic::catch_unwind(|| begin_gpu_query("nested")).is_err());
        sample_gpu_queries();
        end_gpu_query();
        assert!(std::panic::catch_unwind(end_gpu_query).is_err());
        assert!(gpu_queries().is_empty());
        assert_eq!(gpu_query_labels(), ["work"]);
    }

    #[test]
    fn capture_pause_nesting_and_frame_requests_restore_capture() {
        let mut profiler = recording();
        assert!(!profiler.pause_capture());
        profiler.capture_request = true;
        assert_eq!(profiler.advance_frame(0.1, 1.).capture, Some(true));
        assert!(profiler.pause_capture());
        assert!(!profiler.pause_capture());
        assert!(!profiler.resume_capture());
        assert!(profiler.resume_capture());
        assert!(!profiler.resume_capture());
        profiler.pause_capture();
        assert_eq!(profiler.advance_frame(0.1, 2.).capture, Some(false));
        assert!(!profiler.resume_capture());
        assert_eq!(profiler.advance_frame(0.1, 3.).capture, None);
    }
}
