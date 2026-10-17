//! Exercise log-handle changes alongside native file/context operations.
#![cfg(feature = "log")]

use std::sync::{Arc, Barrier};

use gphoto2::{forward_to_log, CameraFile, Context, LogLevel};

#[test]
fn logging_can_start_change_levels_and_stop_during_native_operations() {
    let start = Arc::new(Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|worker| {
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                // Race first registration against both public native-object
                // constructors, then repeatedly change Rust forwarding state.
                for iteration in 0..100 {
                    let logging = if worker % 2 == 0 {
                        Some(forward_to_log(LogLevel::Data).unwrap())
                    } else {
                        None
                    };
                    let context = Context::new().unwrap();
                    let file = CameraFile::from_bytes("test.jpg", "image/jpeg", b"test").unwrap();
                    let extra = forward_to_log(LogLevel::Error).unwrap();
                    assert_eq!(file.data().unwrap(), b"test");
                    assert_eq!(file.name().unwrap(), "test.jpg");
                    if iteration % 2 == 0 {
                        drop(logging);
                        drop(extra);
                    } else {
                        drop(extra);
                        drop(logging);
                    }
                    drop(file);
                    drop(context);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
}
