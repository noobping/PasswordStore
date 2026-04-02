use adw::glib;
use std::cell::RefCell;
use std::rc::Rc;
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

pub fn spawn_result_task_with_finalizer<T, Task, Finalize, HandleResult, HandleDisconnect>(
    task: Task,
    finalize: Finalize,
    handle_result: HandleResult,
    handle_disconnect: HandleDisconnect,
) where
    T: Send + 'static,
    Task: FnOnce() -> T + Send + 'static,
    Finalize: FnOnce() + 'static,
    HandleResult: FnOnce(T) + 'static,
    HandleDisconnect: FnOnce() + 'static,
{
    let finalize = Rc::new(RefCell::new(Some(finalize)));
    let finalize_for_result = finalize.clone();
    let finalize_for_disconnect = finalize;

    spawn_result_task(
        task,
        move |result| {
            if let Some(finalize) = finalize_for_result.borrow_mut().take() {
                finalize();
            }
            handle_result(result);
        },
        move || {
            if let Some(finalize) = finalize_for_disconnect.borrow_mut().take() {
                finalize();
            }
            handle_disconnect();
        },
    );
}

pub fn spawn_progress_result_task<T, P, Task, HandleProgress, HandleResult, HandleDisconnect>(
    task: Task,
    handle_progress: HandleProgress,
    handle_result: HandleResult,
    handle_disconnect: HandleDisconnect,
) where
    T: Send + 'static,
    P: Send + 'static,
    Task: FnOnce(mpsc::Sender<P>) -> T + Send + 'static,
    HandleProgress: FnMut(P) + 'static,
    HandleResult: FnOnce(T) + 'static,
    HandleDisconnect: FnOnce() + 'static,
{
    let (progress_tx, progress_rx) = mpsc::channel::<P>();
    let (result_tx, result_rx) = mpsc::channel::<T>();
    thread::spawn(move || {
        let result = task(progress_tx);
        let _ = result_tx.send(result);
    });

    let mut handle_progress = handle_progress;
    let mut handle_result = Some(handle_result);
    let mut handle_disconnect = Some(handle_disconnect);
    glib::timeout_add_local(
        Duration::from_millis(BACKGROUND_TASK_POLL_INTERVAL_MS),
        move || {
            loop {
                match progress_rx.try_recv() {
                    Ok(progress) => handle_progress(progress),
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                }
            }

            match result_rx.try_recv() {
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
            }
        },
    );
}
