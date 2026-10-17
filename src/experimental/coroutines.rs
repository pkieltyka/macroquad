//! Cooperative tasks polled once per frame, or on an explicit manual timeline.
//!
//! Tasks and their handles belong to the current thread. Futures are polled and
//! destroyed outside the scheduler's borrow, so they may call Macroquad, create
//! other tasks, retrieve results, or cancel tasks (including themselves).

use std::{
    any::{Any, TypeId},
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
};

struct Task {
    future: Pin<Box<dyn Future<Output = Box<dyn Any>>>>,
    manual: bool,
    has_value: bool,
}

enum State {
    Running(Task),
    Polling { cancelled: bool, manual: bool },
    Value(Box<dyn Any>),
    Cancelled,
}

#[derive(Default)]
struct Scheduler {
    next_id: u64,
    tasks: BTreeMap<u64, State>,
}

thread_local! {
    static SCHEDULER: RefCell<Scheduler> = RefCell::new(Scheduler::default());
    static ACTIVE_DELTA: Cell<Option<f64>> = const { Cell::new(None) };
    static SHUTTING_DOWN: Cell<bool> = const { Cell::new(false) };
}

impl Drop for Scheduler {
    fn drop(&mut self) {
        // TLS keys are inaccessible while their destructor runs. Mark the
        // scheduler closed before dropping user futures/results, so their Drop
        // implementations can query/cancel/start without reentering that key.
        let _ = SHUTTING_DOWN.try_with(|closing| closing.set(true));
        let tasks = std::mem::take(&mut self.tasks);
        drop(tasks);
    }
}

fn shutting_down() -> bool {
    SHUTTING_DOWN.try_with(Cell::get).unwrap_or(true)
}

fn completed_unit<T: Any>() -> Option<T> {
    if TypeId::of::<T>() == TypeId::of::<()>() {
        let unit: Box<dyn Any> = Box::new(());
        Some(*unit.downcast::<T>().unwrap())
    } else {
        None
    }
}

fn with_scheduler<R>(f: impl FnOnce(&mut Scheduler) -> R) -> R {
    SCHEDULER.with(|scheduler| f(&mut scheduler.borrow_mut()))
}

/// A copyable handle to a task on the current thread. Dropping a handle leaves
/// the task running; use `stop_coroutine` to cancel it explicitly.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<macroquad::experimental::coroutines::Coroutine>();
/// ```
pub struct Coroutine<T = ()> {
    id: u64,
    _result: PhantomData<fn() -> T>,
    _thread: PhantomData<Rc<()>>,
}

impl<T> Copy for Coroutine<T> {}
impl<T> Clone for Coroutine<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> std::fmt::Debug for Coroutine<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Coroutine").field("id", &self.id).finish()
    }
}

impl<T: Any> Coroutine<T> {
    /// Returns true after completion or cancellation.
    pub fn is_done(&self) -> bool {
        if shutting_down() {
            return true;
        }
        with_scheduler(|scheduler| match scheduler.tasks.get(&self.id) {
            Some(State::Running(_)) => false,
            Some(State::Polling { cancelled, .. }) => *cancelled,
            _ => true,
        })
    }

    /// Takes a completed result once. Unit results remain retrievable after
    /// completion/cancellation without retaining a scheduler allocation.
    pub fn retrieve(&self) -> Option<T> {
        if shutting_down() {
            return completed_unit();
        }
        let (done, value) = with_scheduler(|scheduler| {
            let state = scheduler.tasks.get(&self.id);
            let done = !matches!(
                state,
                Some(State::Running(_))
                    | Some(State::Polling {
                        cancelled: false,
                        ..
                    })
            );
            let value = if matches!(state, Some(State::Value(_))) {
                scheduler.tasks.remove(&self.id)
            } else {
                None
            };
            (done, value)
        });
        if let Some(State::Value(value)) = value {
            return Some(
                *value
                    .downcast::<T>()
                    .expect("Coroutine result type changed"),
            );
        }
        if done {
            completed_unit()
        } else {
            None
        }
    }

    /// Disables automatic polling. Subsequent manual polls advance this task's
    /// local timer by their supplied delta, independently of nested tasks.
    pub fn set_manual_poll(&mut self) {
        if shutting_down() {
            return;
        }
        with_scheduler(|scheduler| match scheduler.tasks.get_mut(&self.id) {
            Some(State::Running(task)) => {
                task.manual = true;
            }
            Some(State::Polling { manual, .. }) => {
                *manual = true;
            }
            _ => {}
        });
    }

    /// Polls a manual task once. Recursive polling of the same task and invalid
    /// time deltas panic before borrowing its future.
    pub fn poll(&mut self, delta_time: f64) {
        assert!(
            delta_time.is_finite() && delta_time >= 0.,
            "Invalid coroutine time delta"
        );
        if !shutting_down() {
            poll_task(self.id, delta_time, true);
        }
    }
}

/// Starts a thread-local task. Its first automatic poll occurs on the next
/// scheduler update; tasks created during an update wait until the following one.
pub fn start_coroutine<T: Any>(future: impl Future<Output = T> + 'static) -> Coroutine<T> {
    if shutting_down() {
        drop(future);
        return Coroutine {
            id: u64::MAX,
            _result: PhantomData,
            _thread: PhantomData,
        };
    }
    // Allocate the ID before moving user data into a scheduler borrow. IDs never
    // reset or wrap, so old handles cannot address newly created tasks.
    let id = with_scheduler(|scheduler| {
        let id = scheduler.next_id;
        id.checked_add(1).map(|next| {
            scheduler.next_id = next;
            id
        })
    })
    .expect("Coroutine identifiers exhausted");
    let task = Task {
        future: Box::pin(async move { Box::new(future.await) as Box<dyn Any> }),
        manual: false,
        has_value: TypeId::of::<T>() != TypeId::of::<()>(),
    };
    with_scheduler(|scheduler| {
        scheduler.tasks.insert(id, State::Running(task));
    });
    Coroutine {
        id,
        _result: PhantomData,
        _thread: PhantomData,
    }
}

/// Cancels a task. If it is currently polling, cancellation takes effect when
/// that poll returns. Future/result destructors run without a scheduler borrow.
pub fn stop_coroutine<T: Any>(coroutine: Coroutine<T>) {
    if shutting_down() {
        return;
    }
    let removed = with_scheduler(|scheduler| {
        if let Some(State::Polling { cancelled, .. }) = scheduler.tasks.get_mut(&coroutine.id) {
            *cancelled = true;
            None
        } else {
            scheduler.tasks.remove(&coroutine.id)
        }
    });
    drop(removed);
}

/// Cancels the tasks present at this call. Tasks started by their destructors
/// remain valid and run on a later update.
pub fn stop_all_coroutines() {
    if shutting_down() {
        return;
    }
    let removed = with_scheduler(|scheduler| {
        let mut removed = Vec::new();
        for state in scheduler.tasks.values_mut() {
            if let State::Polling { cancelled, .. } = state {
                *cancelled = true;
            } else {
                removed.push(std::mem::replace(state, State::Cancelled));
            }
        }
        scheduler
            .tasks
            .retain(|_, state| !matches!(state, State::Cancelled));
        removed
    });
    drop(removed);
}

struct PollGuard {
    id: u64,
    previous_delta: Option<f64>,
    finished: bool,
}

impl Drop for PollGuard {
    fn drop(&mut self) {
        ACTIVE_DELTA.with(|delta| delta.set(self.previous_delta));
        if !self.finished {
            let removed = with_scheduler(|scheduler| scheduler.tasks.remove(&self.id));
            drop(removed);
        }
    }
}

fn poll_task(id: u64, delta: f64, manual_poll: bool) {
    let task = with_scheduler(|scheduler| -> Result<Option<Task>, &'static str> {
        let Some(state) = scheduler.tasks.get_mut(&id) else {
            return Ok(None);
        };
        match state {
            State::Running(task) => {
                if manual_poll && !task.manual {
                    return Err("Coroutine is not manually polled");
                }
                if !manual_poll && task.manual {
                    return Ok(None);
                }
                let polling = State::Polling {
                    cancelled: false,
                    manual: task.manual,
                };
                match std::mem::replace(state, polling) {
                    State::Running(task) => Ok(Some(task)),
                    _ => unreachable!(),
                }
            }
            State::Polling { .. } => {
                if manual_poll {
                    Err("Coroutine is already being polled")
                } else {
                    Ok(None)
                }
            }
            _ => Ok(None),
        }
    })
    .unwrap_or_else(|message| panic!("{message}"));
    let Some(mut task) = task else {
        return;
    };
    // Declared after `task`: restore the placeholder/clock before destroying
    // the future object. Polling can itself drop async locals while unwinding;
    // those destructors also run outside the scheduler borrow.
    let mut guard = PollGuard {
        id,
        previous_delta: ACTIVE_DELTA.with(|active| active.replace(Some(delta))),
        finished: false,
    };
    let result = crate::exec::resume(&mut task.future);
    let polling = with_scheduler(|scheduler| scheduler.tasks.remove(&id));
    if let Some(State::Polling {
        cancelled: false,
        manual,
    }) = polling
    {
        task.manual = manual;
        if let Some(value) = result {
            if task.has_value {
                with_scheduler(|scheduler| {
                    scheduler.tasks.insert(id, State::Value(value));
                });
            }
        } else {
            with_scheduler(|scheduler| {
                scheduler.tasks.insert(id, State::Running(task));
            });
        }
    }
    guard.finished = true;
    drop(guard);
    // Completed/cancelled futures and discarded values drop here, with no borrow.
}

pub(crate) fn update(delta: f64) {
    if shutting_down() {
        return;
    }
    // Miniquad's clock can move backwards after a system clock correction.
    // Automatic timers pause for an invalid frame instead of panicking.
    let delta = if delta.is_finite() { delta.max(0.) } else { 0. };
    let ids = with_scheduler(|scheduler| {
        scheduler
            .tasks
            .iter()
            .filter_map(|(&id, state)| {
                matches!(state, State::Running(Task { manual: false, .. })).then_some(id)
            })
            .collect::<Vec<_>>()
    });
    for id in ids {
        poll_task(id, delta, false);
    }
}

pub(crate) fn allocated_memory() -> usize {
    if shutting_down() {
        return 0;
    }
    with_scheduler(|scheduler| scheduler.tasks.len() * std::mem::size_of::<(u64, State)>())
}

pub(crate) fn active_coroutines_count() -> usize {
    if shutting_down() {
        return 0;
    }
    with_scheduler(|scheduler| scheduler.tasks.len())
}

pub struct TimerDelayFuture {
    remaining_time: f32,
}

impl Future for TimerDelayFuture {
    type Output = Option<()>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let delta = ACTIVE_DELTA
            .with(Cell::get)
            .unwrap_or_else(|| crate::time::get_frame_time() as f64);
        self.remaining_time -= delta as f32;
        if self.remaining_time <= 0. {
            Poll::Ready(Some(()))
        } else {
            Poll::Pending
        }
    }
}

pub const fn wait_seconds(time: f32) -> TimerDelayFuture {
    TimerDelayFuture {
        remaining_time: time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        future::poll_fn,
        panic::{catch_unwind, AssertUnwindSafe},
    };

    struct OnDrop(Option<Box<dyn FnOnce()>>);
    impl OnDrop {
        fn new(f: impl FnOnce() + 'static) -> Self {
            Self(Some(Box::new(f)))
        }
        fn keep_alive(&self) {}
    }
    impl Drop for OnDrop {
        fn drop(&mut self) {
            if let Some(f) = self.0.take() {
                f();
            }
        }
    }

    #[test]
    fn nested_creation_waits_until_the_next_update() {
        let events = Rc::new(RefCell::new(vec![]));
        let output = events.clone();
        let outer = start_coroutine(async move {
            output.borrow_mut().push("outer");
            assert_eq!(crate::telemetry::active_coroutines_count(), 1);
            for _ in 0..64 {
                let output = output.clone();
                start_coroutine(async move {
                    output.borrow_mut().push("inner");
                });
            }
        });
        update(0.01);
        assert!(outer.is_done());
        assert_eq!(*events.borrow(), vec!["outer"]);
        update(0.01);
        assert_eq!(events.borrow().len(), 65);
        assert_eq!(active_coroutines_count(), 0);
    }

    #[test]
    fn results_can_be_retrieved_while_another_task_is_polling() {
        let value = start_coroutine(async { Rc::new(42) });
        update(0.01);
        let reader = start_coroutine(async move {
            assert!(value.is_done());
            assert_eq!(*value.retrieve().unwrap(), 42);
            assert!(value.retrieve().is_none());
        });
        update(0.01);
        assert_eq!(reader.retrieve(), Some(()));
        assert_eq!(active_coroutines_count(), 0);
    }

    #[test]
    fn dropped_handles_detach_but_explicit_stop_releases_captures() {
        let drops = Rc::new(Cell::new(0));
        let count = drops.clone();
        let capture = OnDrop::new(move || count.set(count.get() + 1));
        let task = start_coroutine(async move {
            capture.keep_alive();
            std::future::pending::<()>().await;
        });
        let detached = task;
        let _ = detached;
        update(0.01);
        assert_eq!(drops.get(), 0);
        assert!(!task.is_done());
        stop_coroutine(task);
        assert_eq!(drops.get(), 1);
        assert_eq!(task.retrieve(), Some(()));
        stop_coroutine(task);
        assert_eq!(drops.get(), 1);
    }

    #[test]
    fn self_cancellation_waits_for_the_poll_to_return() {
        let handle = Rc::new(Cell::new(None));
        let self_handle = handle.clone();
        let drops = Rc::new(Cell::new(0));
        let count = drops.clone();
        let capture = OnDrop::new(move || count.set(count.get() + 1));
        let observed = drops.clone();
        let task = start_coroutine(poll_fn(move |_| {
            capture.keep_alive();
            let task: Coroutine = self_handle.get().unwrap();
            stop_coroutine(task);
            assert!(task.is_done());
            assert_eq!(observed.get(), 0);
            Poll::<()>::Pending
        }));
        handle.set(Some(task));
        update(0.01);
        assert_eq!(drops.get(), 1);
        assert_eq!(active_coroutines_count(), 0);
    }

    #[test]
    fn stop_all_and_result_destruction_allow_scheduler_reentry() {
        let events = Rc::new(RefCell::new(vec![]));
        let output = events.clone();
        let result = start_coroutine(async move {
            OnDrop::new(move || {
                output.borrow_mut().push("result dropped");
                start_coroutine(async {});
            })
        });
        update(0.01);
        assert!(result.is_done());
        let output = events.clone();
        let capture = OnDrop::new(move || {
            output.borrow_mut().push("future dropped");
            start_coroutine(async {});
        });
        start_coroutine(async move {
            capture.keep_alive();
            std::future::pending::<()>().await;
        });
        let current = start_coroutine(async {
            stop_all_coroutines();
        });
        update(0.01);
        assert!(current.is_done());
        assert_eq!(events.borrow().len(), 2);
        assert_eq!(active_coroutines_count(), 2);
        update(0.01);
        assert_eq!(active_coroutines_count(), 0);
    }

    #[test]
    fn cancelled_ready_value_is_dropped_without_publishing_it() {
        let handle = Rc::new(Cell::new(None));
        let self_handle = handle.clone();
        let drops = Rc::new(Cell::new(0));
        let count = drops.clone();
        let task = start_coroutine(async move {
            stop_coroutine(self_handle.get().unwrap());
            OnDrop::new(move || {
                assert_eq!(ACTIVE_DELTA.with(Cell::get), None);
                count.set(count.get() + 1);
                start_coroutine(async {});
            })
        });
        handle.set(Some(task));
        update(0.01);
        assert!(task.retrieve().is_none());
        assert_eq!(drops.get(), 1);
        update(0.01);
        assert_eq!(active_coroutines_count(), 0);
    }

    #[test]
    fn stale_ids_never_refer_to_tasks_created_after_clear() {
        let old = start_coroutine(async { 5_u32 });
        stop_all_coroutines();
        let new = start_coroutine(async { 9_u32 });
        stop_coroutine(old);
        update(0.01);
        assert_eq!(old.retrieve(), None);
        assert_eq!(new.retrieve(), Some(9));
        assert_eq!(new.retrieve(), None);
    }

    #[test]
    fn manual_nested_timelines_restore_the_parent_delta() {
        let mut child = start_coroutine(async {
            wait_seconds(2.).await;
        });
        child.set_manual_poll();
        let done = Rc::new(Cell::new(false));
        let completed = done.clone();
        let mut parent = start_coroutine(async move {
            child.poll(0.9);
            wait_seconds(0.15).await;
            completed.set(true);
        });
        parent.set_manual_poll();
        parent.poll(0.1);
        assert!(!done.get());
        update(10.);
        assert!(!done.get());
        parent.poll(0.1);
        assert!(done.get());
        assert_eq!(ACTIVE_DELTA.with(Cell::get), None);
        stop_all_coroutines();
    }

    #[test]
    fn self_poll_is_rejected_without_invalidating_the_outer_future() {
        let handle = Rc::new(Cell::new(None));
        let own_handle = handle.clone();
        let mut task = start_coroutine(poll_fn(move |_| {
            let mut task: Coroutine = own_handle.get().unwrap();
            assert!(catch_unwind(AssertUnwindSafe(|| task.poll(0.01))).is_err());
            Poll::<()>::Ready(())
        }));
        task.set_manual_poll();
        handle.set(Some(task));
        task.poll(0.01);
        assert!(task.is_done());
        assert_eq!(ACTIVE_DELTA.with(Cell::get), None);
    }

    #[test]
    fn poll_unwind_restores_clock_and_allows_capture_drop_reentry() {
        let dropped = Rc::new(Cell::new(false));
        let observed = dropped.clone();
        let capture = OnDrop::new(move || {
            // An async local is destroyed while its poll is unwinding. Its
            // timeline/placeholder are still active, but no store is borrowed.
            assert_eq!(ACTIVE_DELTA.with(Cell::get), Some(0.25));
            assert_eq!(active_coroutines_count(), 1);
            start_coroutine(async {});
            observed.set(true);
        });
        let mut task = start_coroutine(async move {
            capture.keep_alive();
            panic!("task panic");
        });
        task.set_manual_poll();
        assert!(catch_unwind(AssertUnwindSafe(|| task.poll(0.25))).is_err());
        assert!(task.is_done());
        assert!(dropped.get());
        assert_eq!(ACTIVE_DELTA.with(Cell::get), None);
        update(0.01);
        assert_eq!(active_coroutines_count(), 0);
    }

    #[test]
    fn manual_mode_can_be_set_during_a_poll() {
        let handle = Rc::new(Cell::new(None));
        let own_handle = handle.clone();
        let polls = Rc::new(Cell::new(0));
        let count = polls.clone();
        let mut task = start_coroutine(poll_fn(move |_| {
            let mut task: Coroutine = own_handle.get().unwrap();
            task.set_manual_poll();
            count.set(count.get() + 1);
            Poll::<()>::Pending
        }));
        handle.set(Some(task));
        update(0.01);
        update(0.01);
        assert_eq!(polls.get(), 1);
        task.poll(0.01);
        assert_eq!(polls.get(), 2);
        stop_coroutine(task);
    }

    #[test]
    fn invalid_manual_deltas_leave_the_future_unpolled() {
        let polls = Rc::new(Cell::new(0));
        let count = polls.clone();
        let mut task = start_coroutine(async move {
            count.set(count.get() + 1);
        });
        task.set_manual_poll();
        for delta in [-1., f64::NAN, f64::INFINITY] {
            assert!(catch_unwind(AssertUnwindSafe(|| task.poll(delta))).is_err());
        }
        assert_eq!(polls.get(), 0);
        task.poll(0.);
        assert_eq!(polls.get(), 1);
    }

    #[test]
    fn thread_shutdown_drops_captures_without_reentering_destroyed_tls() {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };
        let dropped = Arc::new(AtomicBool::new(false));
        let observed = dropped.clone();
        std::thread::spawn(move || {
            let capture = OnDrop::new(move || {
                stop_all_coroutines();
                assert_eq!(active_coroutines_count(), 0);
                let stopped = start_coroutine(async {});
                assert!(stopped.is_done());
                assert_eq!(stopped.retrieve(), Some(()));
                observed.store(true, Ordering::SeqCst);
            });
            start_coroutine(async move {
                capture.keep_alive();
            });
        })
        .join()
        .unwrap();
        assert!(dropped.load(Ordering::SeqCst));
    }

    #[test]
    fn a_nested_poll_panic_restores_the_parent_timeline() {
        let mut child = start_coroutine(async {
            panic!("nested panic");
        });
        child.set_manual_poll();
        let done = Rc::new(Cell::new(false));
        let completed = done.clone();
        let mut parent = start_coroutine(async move {
            assert!(catch_unwind(AssertUnwindSafe(|| child.poll(0.9))).is_err());
            assert_eq!(ACTIVE_DELTA.with(Cell::get), Some(0.1));
            wait_seconds(0.15).await;
            completed.set(true);
        });
        parent.set_manual_poll();
        parent.poll(0.1);
        assert!(!done.get());
        parent.poll(0.1);
        assert!(done.get());
        assert!(child.is_done());
        assert_eq!(ACTIVE_DELTA.with(Cell::get), None);
    }

    #[test]
    fn automatic_timers_pause_for_backwards_or_invalid_frame_time() {
        let task = start_coroutine(async {
            wait_seconds(0.1).await;
        });
        update(-1.);
        update(f64::NAN);
        assert!(!task.is_done());
        update(0.1);
        assert!(task.is_done());
    }
}
