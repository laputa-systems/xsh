use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use deser::ser::{EventGuard, EventSink, Layer, Next, SerializeDriver, SerializeRef};
use deser::{Atom, Error, ErrorKind, Event, State};

type Seen = (Event<'static>, bool, usize);

struct Capture(Arc<Mutex<Vec<Seen>>>);

impl EventGuard for Capture {
    fn check(&mut self, event: &Event<'_>, state: &State) -> Result<(), Error> {
        self.0.lock().unwrap().push((event.to_static(), state.is_map_key(), state.depth()));
        Ok(())
    }
}

#[derive(Default)]
struct Collect(Vec<Seen>);

impl EventSink for Collect {
    fn event(&mut self, event: Event<'_>, _: SerializeRef<'_>, state: &mut State) -> Result<(), Error> {
        self.0.push((event.to_static(), state.is_map_key(), state.depth()));
        Ok(())
    }

    fn pause(&mut self) -> bool { true }
}

#[test]
fn guards_see_the_same_events_across_driver_entry_points() {
    let value = BTreeMap::from([("items", vec![vec![1u64, 2], vec![3; 1000]])]);
    let mut expected = Vec::new();
    let mut driver = SerializeDriver::new(&value);
    while let Some((event, _, state)) = driver.next().unwrap() {
        expected.push((event.to_static(), state.is_map_key(), state.depth()));
    }
    for entry in 0..4 {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut driver = SerializeDriver::new(&value);
        driver.push_guard(Capture(Arc::clone(&seen)));
        let mut sink = Collect::default();
        match entry {
            0 => while let Some((event, _, state)) = driver.next().unwrap() {
                sink.0.push((event.to_static(), state.is_map_key(), state.depth()));
            },
            1 => driver.drive(|event, state| {
                sink.0.push((event.to_static(), state.is_map_key(), state.depth()));
                Ok(())
            }).unwrap(),
            2 => driver.drive_sink(&mut sink).unwrap(),
            _ => while !driver.drive_until(&mut sink).unwrap() {},
        }
        assert_eq!(*seen.lock().unwrap(), expected);
        assert_eq!(sink.0, expected);
    }
}

struct RejectTwo;

impl EventGuard for RejectTwo {
    fn check(&mut self, event: &Event<'_>, _: &State) -> Result<(), Error> {
        if matches!(event, Event::Atom(Atom::U64(2))) {
            return Err(Error::new(ErrorKind::LimitExceeded, "rejected two"));
        }
        Ok(())
    }
}

struct Double;

impl Layer for Double {
    fn event(&mut self, event: Event<'_>, next: &mut Next<'_>) -> Result<(), Error> {
        if let Event::Atom(atom) = event {
            next.emit(Event::Atom(atom.clone()))?;
            next.emit(Event::Atom(atom))
        } else {
            next.emit(event)
        }
    }
}

#[test]
fn guards_reject_original_events_before_layers_and_stop_delivery() {
    let value = vec![1u64, 2, 3];
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut driver = SerializeDriver::new(&value);
    driver.push_layer(Double);
    driver.push_guard(RejectTwo);
    driver.push_guard(Capture(Arc::clone(&seen)));
    let mut delivered = Vec::new();
    let error = driver.drive(|event, _| {
        delivered.push(event.to_static());
        Ok(())
    }).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::LimitExceeded);
    assert!(error.to_string().contains("rejected two"));
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(matches!(seen[0].0, Event::SeqStart(_)));
    assert_eq!(seen[1].0, 1u64.into());
    assert!(matches!(delivered[0], Event::SeqStart(_)));
    assert_eq!(delivered[1..], [1u64.into(), 1u64.into()]);
}

#[test]
fn next_rejects_an_event_before_returning_it() {
    let value = vec![1u64, 2, 3];
    let mut driver = SerializeDriver::new(&value);
    driver.push_guard(RejectTwo);
    assert!(matches!(driver.next().unwrap(), Some((Event::SeqStart(_), _, _))));
    assert!(matches!(driver.next().unwrap(), Some((Event::Atom(Atom::U64(1)), _, _))));
    assert_eq!(driver.next().unwrap_err().kind(), ErrorKind::LimitExceeded);
}

#[test]
fn sinks_reject_an_event_before_delivery() {
    let value = vec![1u64, 2, 3];
    for pausable in [false, true] {
        let mut driver = SerializeDriver::new(&value);
        driver.push_guard(RejectTwo);
        let mut sink = Collect::default();
        let error = if pausable {
            loop {
                match driver.drive_until(&mut sink) {
                    Ok(false) => {},
                    Ok(true) => panic!("the guard must reject two"),
                    Err(error) => break error,
                }
            }
        } else {
            driver.drive_sink(&mut sink).unwrap_err()
        };
        assert_eq!(error.kind(), ErrorKind::LimitExceeded);
        assert_eq!(sink.0.len(), 2);
        assert_eq!(sink.0[1].0, 1u64.into());
    }
}

#[test]
fn rejection_releases_the_active_borrowing_emitter() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use deser::ser::{Emit, SeqEmitter, SerializeHandle};

    struct Root {
        items: [u64; 3],
        drops: Arc<AtomicUsize>,
    }

    struct Items<'a> {
        items: std::slice::Iter<'a, u64>,
        drops: Arc<AtomicUsize>,
    }

    impl Drop for Items<'_> {
        fn drop(&mut self) { self.drops.fetch_add(1, Ordering::Relaxed); }
    }

    impl SeqEmitter for Items<'_> {
        fn next(&mut self, _: &mut State) -> Result<Option<SerializeHandle<'_>>, Error> {
            Ok(self.items.next().map(SerializeHandle::to))
        }
    }

    impl deser::Serialize for Root {
        fn serialize<'a>(value: &'a Self, state: &mut State) -> Result<Emit<'a>, Error> {
            Ok(Emit::seq(Items { items: value.items.iter(), drops: Arc::clone(&value.drops) }, state))
        }
    }

    let drops = Arc::new(AtomicUsize::new(0));
    let root = Root { items: [1, 2, 3], drops: Arc::clone(&drops) };
    let mut driver = SerializeDriver::new(&root);
    driver.push_guard(RejectTwo);
    assert_eq!(driver.drive(|_, _| Ok(())).unwrap_err().kind(), ErrorKind::LimitExceeded);
    drop(driver);
    assert_eq!(drops.load(Ordering::Relaxed), 1);
    assert_eq!(root.items, [1, 2, 3]);
}
