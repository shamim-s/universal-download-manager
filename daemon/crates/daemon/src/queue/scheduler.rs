//! Priority queue + concurrency limiter (Phase 5).
//!
//! A single scheduler task owns all queue state (so there are no races). It
//! dispatches the highest-priority queued job whenever a slot is free and the
//! optional schedule window allows it. Downloads signal `SlotFreed` when they
//! end (complete / fail / pause), which lets the next job start.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashSet};
use std::sync::Arc;

use chrono::{Datelike, Local};
use tokio::sync::mpsc::UnboundedReceiver;
use uuid::Uuid;

use udm_storage::db;
use udm_storage::models::DownloadJob;

use crate::bridge;
use crate::config::settings::{self, ScheduleWindow};
use crate::state::AppState;

/// Commands accepted by the scheduler task.
pub enum SchedulerCmd {
    /// Queue a job to run when a slot is available.
    Enqueue(Box<DownloadJob>),
    /// A running job finished/paused; free its slot.
    SlotFreed(Uuid),
    /// Change the max concurrent downloads.
    SetMaxConcurrent(u8),
    /// Re-prioritise a queued job (and persist the new priority).
    SetPriority(Uuid, u8),
    /// Remove a job from the pending queue (e.g. cancelled before starting).
    RemovePending(Uuid),
}

/// One entry in the pending priority queue: higher priority first, FIFO on ties.
struct Queued {
    priority: u8,
    seq: u64,
    job: DownloadJob,
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.seq == other.seq
    }
}
impl Eq for Queued {}
impl Ord for Queued {
    fn cmp(&self, other: &Self) -> Ordering {
        // Max-heap: higher priority wins; for equal priority, smaller seq wins.
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}
impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

struct Scheduler {
    max: usize,
    seq: u64,
    running: HashSet<Uuid>,
    pending: BinaryHeap<Queued>,
    window: Option<ScheduleWindow>,
}

impl Scheduler {
    fn new(max: u8) -> Self {
        Self {
            max: max.max(1) as usize,
            seq: 0,
            running: HashSet::new(),
            pending: BinaryHeap::new(),
            window: None,
        }
    }

    fn push(&mut self, job: DownloadJob) {
        self.seq += 1;
        self.pending.push(Queued {
            priority: job.priority,
            seq: self.seq,
            job,
        });
    }

    fn schedule_ok(&self) -> bool {
        match &self.window {
            None => true,
            Some(w) => {
                let now = Local::now();
                settings::is_within(now.time(), now.date_naive().weekday(), w)
            }
        }
    }

    /// Start as many queued jobs as slots (and the window) allow.
    fn dispatch(&mut self, state: &Arc<AppState>) {
        while self.running.len() < self.max && self.schedule_ok() {
            match self.pending.pop() {
                Some(q) => {
                    self.running.insert(q.job.id);
                    bridge::start_download(Arc::clone(state), q.job);
                }
                None => break,
            }
        }
    }

    fn set_priority(&mut self, state: &Arc<AppState>, id: Uuid, priority: u8) {
        // Persist regardless of where the job is.
        if let Ok(conn) = state.db.lock() {
            if let Ok(Some(mut job)) = db::get_job(&conn, &id) {
                job.priority = priority;
                let _ = db::insert_job(&conn, &job);
            }
        }
        // If queued, rebuild the heap with the new priority.
        if self.pending.iter().any(|q| q.job.id == id) {
            let mut items: Vec<Queued> = self.pending.drain().collect();
            for q in &mut items {
                if q.job.id == id {
                    q.priority = priority;
                    q.job.priority = priority;
                }
            }
            self.pending = items.into_iter().collect();
        }
    }

    fn remove_pending(&mut self, id: Uuid) -> bool {
        let before = self.pending.len();
        let kept: Vec<Queued> = self.pending.drain().filter(|q| q.job.id != id).collect();
        self.pending = kept.into_iter().collect();
        self.pending.len() != before
    }
}

/// Run the scheduler loop until the command channel closes.
pub async fn run(
    state: Arc<AppState>,
    mut rx: UnboundedReceiver<SchedulerCmd>,
    max_concurrent: u8,
) {
    let mut sched = Scheduler::new(max_concurrent);
    tracing::info!("scheduler started (max concurrent: {})", sched.max);
    while let Some(cmd) = rx.recv().await {
        match cmd {
            SchedulerCmd::Enqueue(job) => {
                sched.push(*job);
                sched.dispatch(&state);
            }
            SchedulerCmd::SlotFreed(id) => {
                sched.running.remove(&id);
                sched.dispatch(&state);
            }
            SchedulerCmd::SetMaxConcurrent(n) => {
                sched.max = (n as usize).max(1);
                sched.dispatch(&state);
            }
            SchedulerCmd::SetPriority(id, p) => sched.set_priority(&state, id, p),
            SchedulerCmd::RemovePending(id) => {
                sched.remove_pending(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_then_fifo_ordering() {
        let mut h: BinaryHeap<Queued> = BinaryHeap::new();
        let mk = |prio, seq| Queued {
            priority: prio,
            seq,
            job: sample(prio),
        };
        h.push(mk(100, 1));
        h.push(mk(200, 2));
        h.push(mk(100, 3));
        h.push(mk(200, 4));
        // 200/seq2, 200/seq4, 100/seq1, 100/seq3
        assert_eq!(
            (h.pop().unwrap().priority, h.pop().unwrap().priority),
            (200, 200)
        );
        assert_eq!(h.pop().unwrap().seq, 1);
        assert_eq!(h.pop().unwrap().seq, 3);
    }

    fn sample(priority: u8) -> DownloadJob {
        use chrono::Utc;
        DownloadJob {
            id: Uuid::new_v4(),
            url: "http://x/y".into(),
            filename: "y".into(),
            save_path: "y".into(),
            file_size: None,
            downloaded_bytes: 0,
            status: udm_storage::models::JobStatus::Queued,
            priority,
            created_at: Utc::now(),
            completed_at: None,
            error: None,
            referrer: None,
            cookies: None,
            user_agent: "t".into(),
            headers: Default::default(),
            segments: vec![],
            checksum: None,
            source_browser: "t".into(),
            tags: vec![],
        }
    }
}
