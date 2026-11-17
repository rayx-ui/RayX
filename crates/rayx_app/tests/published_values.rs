//! Published values: a `StateCell` keeps the latest value for any thread and notifies its
//! subscribers in order; an `EventStream` reaches its current subscribers and keeps nothing.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use rayx_app::{EventStream, StateCell};

/// A log the test reads and a subscriber writes.
type Log<T> = Arc<Mutex<Vec<T>>>;

fn log<T: Clone + Send + 'static>() -> (Log<T>, Log<T>) {
    let log = Arc::new(Mutex::new(Vec::new()));
    (Arc::clone(&log), log)
}

#[test]
fn a_cell_starts_at_its_initial_value_and_keeps_the_latest() {
    let cell = StateCell::new(1_u32);
    assert_eq!(*cell.get(), 1);
    cell.publish(2);
    cell.publish(3);
    assert_eq!(*cell.get(), 3);
    // Clones share the value.
    let shared = cell.clone();
    shared.publish(4);
    assert_eq!(*cell.get(), 4);
}

#[test]
fn a_value_published_on_one_thread_is_read_on_another() {
    let cell = StateCell::new(String::from("en"));
    let writer = cell.clone();
    thread::spawn(move || writer.publish("hr".into()))
        .join()
        .unwrap();
    assert_eq!(*cell.get(), "hr");
    let reader = cell.clone();
    let read = thread::spawn(move || reader.get().to_string())
        .join()
        .unwrap();
    assert_eq!(read, "hr");
}

#[test]
fn subscribers_hear_each_publish_in_subscription_order() {
    let cell = StateCell::new(0_u32);
    let (seen, log) = log::<String>();
    let first = log.clone();
    let _a = cell.subscribe(move |value| first.lock().unwrap().push(format!("a{value}")));
    let second = log.clone();
    let _b = cell.subscribe(move |value| second.lock().unwrap().push(format!("b{value}")));
    cell.publish(1);
    cell.publish(2);
    assert_eq!(*seen.lock().unwrap(), ["a1", "b1", "a2", "b2"]);
    assert_eq!(cell.subscribers(), 2);
}

#[test]
fn dropping_a_subscription_stops_it_and_detach_keeps_it() {
    let cell = StateCell::new(0_u32);
    let (seen, log) = log::<u32>();
    let kept = log.clone();
    cell.subscribe(move |value| kept.lock().unwrap().push(**value))
        .detach();
    let dropped = log.clone();
    let subscription = cell.subscribe(move |value| dropped.lock().unwrap().push(100 + **value));
    cell.publish(1);
    drop(subscription);
    cell.publish(2);
    assert_eq!(*seen.lock().unwrap(), [1, 101, 2]);
    assert_eq!(cell.subscribers(), 1);
}

#[test]
fn a_subscriber_may_read_publish_and_subscribe_from_its_callback() {
    let cell = StateCell::new(0_u32);
    let (seen, log) = log::<u32>();
    let inner = cell.clone();
    let later: Arc<Mutex<Vec<rayx_app::ChangeSubscription>>> = Arc::default();
    let keep = Arc::clone(&later);
    let _subscription = cell.subscribe(move |value| {
        // Reading inside the callback sees the value just published.
        log.lock().unwrap().push(*inner.get());
        if **value < 3 {
            // Publishing again from the callback does not deadlock.
            inner.publish(**value + 1);
        }
        if **value == 1 {
            keep.lock().unwrap().push(inner.subscribe(|_| {}));
        }
    });
    cell.publish(1);
    assert_eq!(*cell.get(), 3);
    assert_eq!(*seen.lock().unwrap(), [1, 2, 3]);
    assert_eq!(cell.subscribers(), 2);
}

#[test]
fn a_publish_from_a_callback_reaches_every_subscriber_after_the_one_it_came_from() {
    let cell = StateCell::new(0_u32);
    let (first_seen, first_log) = log::<u32>();
    let (second_seen, second_log) = log::<u32>();
    let inner = cell.clone();
    let _first = cell.subscribe(move |value| {
        first_log.lock().unwrap().push(**value);
        if **value < 3 {
            inner.publish(**value + 1);
        }
    });
    let _second = cell.subscribe(move |value| second_log.lock().unwrap().push(**value));
    cell.publish(1);
    assert_eq!(*cell.get(), 3);
    // Each subscriber hears 1, 2, 3 in order: the last value it heard is the cell's.
    assert_eq!(*first_seen.lock().unwrap(), [1, 2, 3]);
    assert_eq!(*second_seen.lock().unwrap(), [1, 2, 3]);
}

#[test]
fn under_concurrent_publishes_every_subscriber_ends_on_the_cell_s_value() {
    for _ in 0..20 {
        let cell = StateCell::new(0_u32);
        let logs: Vec<Log<u32>> = (0..3).map(|_| Arc::default()).collect();
        let _subscriptions: Vec<_> = logs
            .iter()
            .map(|log| {
                let log = Arc::clone(log);
                cell.subscribe(move |value| log.lock().unwrap().push(**value))
            })
            .collect();
        let threads: Vec<_> = (1..=4_u32)
            .map(|n| {
                let cell = cell.clone();
                thread::spawn(move || {
                    for i in 0..50 {
                        cell.publish(n * 1000 + i);
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let last = *cell.get();
        let first = logs[0].lock().unwrap().clone();
        assert_eq!(first.len(), 200);
        for log in &logs {
            let heard = log.lock().unwrap();
            // One order for everyone, ending on the cell's value.
            assert_eq!(*heard, first);
            assert_eq!(heard.last(), Some(&last));
        }
    }
}

#[test]
fn an_event_published_from_a_callback_follows_the_one_being_delivered() {
    let stream = EventStream::<u32>::new();
    let (seen, log) = log::<String>();
    let again = stream.clone();
    let first = log.clone();
    let _a = stream.subscribe(move |event| {
        first.lock().unwrap().push(format!("a{event}"));
        if *event == 1 {
            again.publish(2);
        }
    });
    let second = log.clone();
    let _b = stream.subscribe(move |event| second.lock().unwrap().push(format!("b{event}")));
    stream.publish(1);
    assert_eq!(*seen.lock().unwrap(), ["a1", "b1", "a2", "b2"]);
}

#[test]
fn concurrent_publishes_leave_one_of_them_and_notify_each_once() {
    let cell = StateCell::new(0_u32);
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let _subscription = cell.subscribe(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    let threads: Vec<_> = (1..=8_u32)
        .map(|n| {
            let cell = cell.clone();
            thread::spawn(move || {
                for i in 0..100 {
                    cell.publish(n * 1000 + i);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 800);
    // The value is one a thread published last.
    let last = *cell.get();
    assert_eq!(last % 1000, 99, "{last}");
    // With the threads done, a last publish is the value.
    cell.publish(7);
    assert_eq!(*cell.get(), 7);
}

#[test]
fn events_reach_the_current_subscribers_only_and_are_not_kept() {
    let stream = EventStream::<String>::new();
    // Nobody hears an event published before subscribing.
    stream.publish("early".into());
    let (seen, log) = log::<String>();
    let first = log.clone();
    let a = stream.subscribe(move |event| first.lock().unwrap().push(format!("a:{event}")));
    stream.publish("one".into());
    let second = log.clone();
    let _b = stream.subscribe(move |event| second.lock().unwrap().push(format!("b:{event}")));
    stream.publish("two".into());
    drop(a);
    stream.publish("three".into());
    assert_eq!(
        *seen.lock().unwrap(),
        ["a:one", "a:two", "b:two", "b:three"]
    );
}

#[test]
fn events_from_other_threads_reach_each_subscriber_once() {
    let stream = EventStream::<u32>::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let _subscription = stream.subscribe(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
    });
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let stream = stream.clone();
            thread::spawn(move || {
                for i in 0..50 {
                    stream.publish(i);
                }
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap();
    }
    assert_eq!(calls.load(Ordering::SeqCst), 200);
}

#[test]
fn no_publish_is_left_undelivered_under_contention() {
    for _ in 0..3000 {
        let cell = StateCell::new(0_u32);
        let stream = EventStream::<u32>::new();
        let heard: Log<u32> = Arc::default();
        let events = Arc::new(AtomicUsize::new(0));
        let log = Arc::clone(&heard);
        let _state = cell.subscribe(move |value| log.lock().unwrap().push(**value));
        let counted = Arc::clone(&events);
        let _events = stream.subscribe(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
        });
        let threads: Vec<_> = (1..=8_u32)
            .map(|n| {
                let (cell, stream) = (cell.clone(), stream.clone());
                thread::spawn(move || {
                    for i in 0..3 {
                        cell.publish(n * 10 + i);
                        stream.publish(i);
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        let heard = heard.lock().unwrap();
        assert_eq!(heard.len(), 24, "every publish delivered");
        assert_eq!(
            heard.last(),
            Some(&*cell.get()),
            "the last heard is the cell's"
        );
        assert_eq!(events.load(Ordering::SeqCst), 24, "every event delivered");
    }
}
