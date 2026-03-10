use super::super::store::log_error;
use std::io;
use std::process::{Child, ExitStatus};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Clone, Default)]
pub struct CommandControl {
    child: Arc<Mutex<Option<Child>>>,
}

impl CommandControl {
    pub(super) fn set_child(&self, child: Child) {
        match self.child.lock() {
            Ok(mut slot) => *slot = Some(child),
            Err(poisoned) => {
                let mut slot = poisoned.into_inner();
                *slot = Some(child);
            }
        }
    }

    pub(super) fn clear(&self) {
        match self.child.lock() {
            Ok(mut slot) => {
                slot.take();
            }
            Err(poisoned) => {
                let mut slot = poisoned.into_inner();
                slot.take();
            }
        }
    }

    pub(super) fn wait(&self, context: &str, command: &str) -> io::Result<ExitStatus> {
        loop {
            let status = match self.child.lock() {
                Ok(mut slot) => Self::try_wait_locked(&mut slot),
                Err(poisoned) => {
                    let mut slot = poisoned.into_inner();
                    Self::try_wait_locked(&mut slot)
                }
            };

            match status {
                Ok(Some(status)) => {
                    self.clear();
                    return Ok(status);
                }
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(err) => {
                    self.clear();
                    log_error(format!("{context}\n$ {command}\nfailed to wait: {err}"));
                    return Err(err);
                }
            }
        }
    }

    fn try_wait_locked(child: &mut Option<Child>) -> io::Result<Option<ExitStatus>> {
        let Some(child) = child.as_mut() else {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "command handle missing child process",
            ));
        };
        child.try_wait()
    }
}
