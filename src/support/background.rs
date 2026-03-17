use adw::glib;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;
use std::thread;
use std::time::Duration;

const BACKGROUND_TASK_POLL_INTERVAL_MS: u64 = 50;

pub fn spawn_result_task<T, Task, HandleResult, HandleDisconnect>(
    task: Task,
    handle_result: HandleResult,
    handle_disconnect: HandleDisconnect,
) where
    T: Send + 'static,
    Task: FnOnce() -> T + Send + 'static,
    HandleResult: FnOnce(T) + 'static,
    HandleDisconnect: FnOnce() + 'static,
{
    let (tx, rx) = mpsc::channel::<T>();
    thread::spawn(move || {
        let _ = tx.send(task());
    });

    let mut handle_result = Some(handle_result);
    let mut handle_disconnect = Some(handle_disconnect);
    glib::timeout_add_local(
        Duration::from_millis(BACKGROUND_TASK_POLL_INTERVAL_MS),
        move || match rx.try_recv() {
            Ok(result) => {
                if let Some(handle_result) = handle_result.take() {
                    handle_result(result);
                }
                glib::ControlFlow::Break
            }
            Err(TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(TryRecvError::Disconnected) => {
                if let Some(handle_disconnect) = handle_disconnect.take() {
                    handle_disconnect();
                }
                glib::ControlFlow::Break
            }
        },
    );
}
