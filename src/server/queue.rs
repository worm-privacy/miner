use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::sync::mpsc::{Receiver, Sender, channel, error::TrySendError};
use uuid::Uuid;

use crate::server::types::ProofInput;
#[derive(Clone)]
pub struct JobQueue {
    sender: Sender<(Uuid, ProofInput)>,
    queued: Arc<AtomicUsize>,      // number of jobs waiting in channel
    in_progress: Arc<AtomicUsize>, // 0 or 1 (sequential worker)
}
impl JobQueue {
    pub fn new() -> (Self, Receiver<(Uuid, ProofInput)>) {
        Self::with_capacity(64)
    }

    pub fn with_capacity(capacity: usize) -> (Self, Receiver<(Uuid, ProofInput)>) {
        let (tx, rx) = channel(capacity);
        (
            JobQueue {
                sender: tx,
                queued: Arc::new(AtomicUsize::new(0)),
                in_progress: Arc::new(AtomicUsize::new(0)),
            },
            rx,
        )
    }
    pub fn submit(&self, job_id: Uuid, input: ProofInput) -> Result<(), QueueError> {
        self.sender
            .try_send((job_id, input))
            .map_err(QueueError::from)?;
        self.queued.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    pub fn queued_len(&self) -> usize {
        self.queued.load(Ordering::Relaxed)
    }
    pub fn in_progress(&self) -> usize {
        self.in_progress.load(Ordering::Relaxed)
    }

    pub fn dec_queued(&self) {
        self.queued.fetch_sub(1, Ordering::SeqCst);
    }
    pub fn inc_in_progress(&self) {
        self.in_progress.fetch_add(1, Ordering::SeqCst);
    }
    pub fn dec_in_progress(&self) {
        self.in_progress.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Debug)]
pub enum QueueError {
    Full,
    Closed,
}

impl From<TrySendError<(Uuid, ProofInput)>> for QueueError {
    fn from(e: TrySendError<(Uuid, ProofInput)>) -> Self {
        match e {
            TrySendError::Full(_) => QueueError::Full,
            TrySendError::Closed(_) => QueueError::Closed,
        }
    }
}
