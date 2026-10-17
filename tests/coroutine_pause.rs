use macroquad::{
    experimental::coroutines::{start_coroutine, wait_seconds},
    window::next_frame,
};

#[macroquad::test]
async fn coroutine_execution_order() {
    start_coroutine(async move {
        println!("a");
        next_frame().await;
        println!("b");
    });
    println!("c");
    next_frame().await;
    println!("d");
    next_frame().await;
}

#[macroquad::test]
async fn coroutine_manual_poll() {
    let player = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));
    let task_player = player.clone();

    let mut coroutine = start_coroutine(async move {
        loop {
            task_player.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            next_frame().await;
        }
    });

    // make sure that coroutine is not yet polled
    assert_eq!(player.load(std::sync::atomic::Ordering::Relaxed), 0);

    coroutine.set_manual_poll();

    // still not polled
    assert_eq!(player.load(std::sync::atomic::Ordering::Relaxed), 0);

    coroutine.poll(0.1);
    assert_eq!(player.load(std::sync::atomic::Ordering::Relaxed), 1);

    next_frame().await;
    next_frame().await;

    // make sure that after main loop's next_frame coroutine was not polled
    assert_eq!(player.load(std::sync::atomic::Ordering::Relaxed), 1);

    // and that we still can poll
    coroutine.poll(0.1);
    assert_eq!(player.load(std::sync::atomic::Ordering::Relaxed), 2);
}

#[macroquad::test]
async fn coroutine_manual_poll_delay() {
    let player = std::sync::Arc::new(std::sync::atomic::AtomicI32::new(0));
    let task_player = player.clone();

    let mut coroutine = start_coroutine(async move {
        wait_seconds(1.).await;
        task_player.store(1, std::sync::atomic::Ordering::Relaxed);
    });

    coroutine.set_manual_poll();

    assert_eq!(player.load(std::sync::atomic::Ordering::Relaxed), 0);

    // not 1 second yet, coroutine will have "now": 0.0, "delta": 0.9, (0.0 + 0.9) < 1.0
    coroutine.poll(0.9);

    assert_eq!(player.load(std::sync::atomic::Ordering::Relaxed), 0);

    // coroutine will have "now": 0.1, delta: 0.11, (0.9 + 0.11) > 1.0, wait_for_seconds pass
    coroutine.poll(0.11);

    assert_eq!(player.load(std::sync::atomic::Ordering::Relaxed), 1);
}
