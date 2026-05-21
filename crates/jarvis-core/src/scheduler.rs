use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{JarvisError, JarvisResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerKind {
    Manual,
    OnceAt { run_at: DateTime<Utc> },
    Interval { every_seconds: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchedulerJobStatus {
    Scheduled,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerJobSpec {
    pub name: String,
    pub command: String,
    pub trigger: TriggerKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerJob {
    pub id: Uuid,
    pub name: String,
    pub command: String,
    pub trigger: TriggerKind,
    pub status: SchedulerJobStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancellation_reason: Option<String>,
}

impl SchedulerJob {
    fn new(spec: SchedulerJobSpec) -> JarvisResult<Self> {
        if spec.name.trim().is_empty() {
            return Err(JarvisError::Validation(
                "scheduler job name cannot be empty".to_string(),
            ));
        }

        if spec.command.trim().is_empty() {
            return Err(JarvisError::Validation(
                "scheduler job command cannot be empty".to_string(),
            ));
        }

        if matches!(spec.trigger, TriggerKind::Interval { every_seconds: 0 }) {
            return Err(JarvisError::Validation(
                "scheduler interval must be greater than zero seconds".to_string(),
            ));
        }

        let now = Utc::now();

        Ok(Self {
            id: Uuid::new_v4(),
            name: spec.name,
            command: spec.command,
            trigger: spec.trigger,
            status: SchedulerJobStatus::Scheduled,
            created_at: now,
            updated_at: now,
            cancelled_at: None,
            cancellation_reason: None,
        })
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            SchedulerJobStatus::Completed
                | SchedulerJobStatus::Cancelled
                | SchedulerJobStatus::Failed
        )
    }

    fn is_due_at(&self, now: DateTime<Utc>) -> bool {
        if self.status != SchedulerJobStatus::Scheduled {
            return false;
        }

        match self.trigger {
            TriggerKind::Manual => true,
            TriggerKind::OnceAt { run_at } => run_at <= now,
            TriggerKind::Interval { every_seconds } => {
                let Ok(seconds) = i64::try_from(every_seconds) else {
                    return false;
                };
                self.updated_at + Duration::seconds(seconds) <= now
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Scheduler {
    jobs: Arc<Mutex<BTreeMap<Uuid, SchedulerJob>>>,
}

impl Scheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_jobs(jobs: Vec<SchedulerJob>) -> Self {
        Self {
            jobs: Arc::new(Mutex::new(
                jobs.into_iter().map(|job| (job.id, job)).collect(),
            )),
        }
    }

    pub fn schedule(&self, spec: SchedulerJobSpec) -> JarvisResult<SchedulerJob> {
        let job = SchedulerJob::new(spec)?;
        self.jobs
            .lock()
            .expect("scheduler jobs lock poisoned")
            .insert(job.id, job.clone());
        Ok(job)
    }

    pub fn list(&self) -> Vec<SchedulerJob> {
        self.jobs
            .lock()
            .expect("scheduler jobs lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    pub fn due_jobs(&self, now: DateTime<Utc>, limit: usize) -> Vec<SchedulerJob> {
        if limit == 0 {
            return Vec::new();
        }

        let mut due = self
            .jobs
            .lock()
            .expect("scheduler jobs lock poisoned")
            .values()
            .filter(|job| job.is_due_at(now))
            .cloned()
            .collect::<Vec<_>>();
        due.sort_by_key(|job| (job.updated_at, job.created_at, job.id));
        due.truncate(limit);
        due
    }

    pub fn get(&self, id: Uuid) -> Option<SchedulerJob> {
        self.jobs
            .lock()
            .expect("scheduler jobs lock poisoned")
            .get(&id)
            .cloned()
    }

    pub fn mark_running(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let mut jobs = self.jobs.lock().expect("scheduler jobs lock poisoned");
        let job = jobs
            .get_mut(&id)
            .ok_or_else(|| JarvisError::Validation(format!("unknown scheduler job: {id}")))?;

        if job.is_terminal() {
            return Err(JarvisError::Validation(format!(
                "terminal scheduler job cannot be marked running: {id}"
            )));
        }

        let now = Utc::now();
        job.status = SchedulerJobStatus::Running;
        job.updated_at = now;

        Ok(job.clone())
    }

    pub fn complete(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let mut jobs = self.jobs.lock().expect("scheduler jobs lock poisoned");
        let job = jobs
            .get_mut(&id)
            .ok_or_else(|| JarvisError::Validation(format!("unknown scheduler job: {id}")))?;

        if job.is_terminal() {
            return Err(JarvisError::Validation(format!(
                "terminal scheduler job cannot be completed: {id}"
            )));
        }

        let now = Utc::now();
        job.status = SchedulerJobStatus::Completed;
        job.updated_at = now;

        Ok(job.clone())
    }

    pub fn fail(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let mut jobs = self.jobs.lock().expect("scheduler jobs lock poisoned");
        let job = jobs
            .get_mut(&id)
            .ok_or_else(|| JarvisError::Validation(format!("unknown scheduler job: {id}")))?;

        if job.is_terminal() {
            return Err(JarvisError::Validation(format!(
                "terminal scheduler job cannot be failed: {id}"
            )));
        }

        let now = Utc::now();
        job.status = SchedulerJobStatus::Failed;
        job.updated_at = now;

        Ok(job.clone())
    }

    pub fn reschedule_interval(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let mut jobs = self.jobs.lock().expect("scheduler jobs lock poisoned");
        let job = jobs
            .get_mut(&id)
            .ok_or_else(|| JarvisError::Validation(format!("unknown scheduler job: {id}")))?;

        if !matches!(job.trigger, TriggerKind::Interval { .. }) {
            return Err(JarvisError::Validation(format!(
                "non-interval scheduler job cannot be rescheduled: {id}"
            )));
        }

        if job.is_terminal() {
            return Err(JarvisError::Validation(format!(
                "terminal scheduler job cannot be rescheduled: {id}"
            )));
        }

        let now = Utc::now();
        job.status = SchedulerJobStatus::Scheduled;
        job.updated_at = now;

        Ok(job.clone())
    }

    pub fn cancel(&self, id: Uuid, reason: impl Into<String>) -> JarvisResult<SchedulerJob> {
        let mut jobs = self.jobs.lock().expect("scheduler jobs lock poisoned");
        let job = jobs
            .get_mut(&id)
            .ok_or_else(|| JarvisError::Validation(format!("unknown scheduler job: {id}")))?;

        if job.status == SchedulerJobStatus::Cancelled {
            return Ok(job.clone());
        }

        if job.is_terminal() {
            return Err(JarvisError::Validation(format!(
                "terminal scheduler job cannot be cancelled: {id}"
            )));
        }

        let now = Utc::now();
        job.status = SchedulerJobStatus::Cancelled;
        job.updated_at = now;
        job.cancelled_at = Some(now);
        job.cancellation_reason = Some(reason.into());

        Ok(job.clone())
    }

    pub fn cancel_active_jobs(&self, reason: impl Into<String>) -> Vec<SchedulerJob> {
        let reason = reason.into();
        let now = Utc::now();
        let mut jobs = self.jobs.lock().expect("scheduler jobs lock poisoned");
        let mut cancelled = Vec::new();

        for job in jobs.values_mut() {
            if matches!(
                job.status,
                SchedulerJobStatus::Scheduled | SchedulerJobStatus::Running
            ) {
                job.status = SchedulerJobStatus::Cancelled;
                job.updated_at = now;
                job.cancelled_at = Some(now);
                job.cancellation_reason = Some(reason.clone());
                cancelled.push(job.clone());
            }
        }

        cancelled
    }

    pub fn cancel_active(&self, reason: impl Into<String>) -> usize {
        self.cancel_active_jobs(reason).len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedules_and_cancels_inspectable_jobs() {
        let scheduler = Scheduler::new();
        let job = scheduler
            .schedule(SchedulerJobSpec {
                name: "morning check".to_string(),
                command: "summarize inbox".to_string(),
                trigger: TriggerKind::Interval { every_seconds: 900 },
            })
            .expect("job should schedule");

        assert_eq!(scheduler.list().len(), 1);
        assert_eq!(scheduler.get(job.id).expect("job").name, "morning check");

        let cancelled = scheduler
            .cancel(job.id, "user requested")
            .expect("job should cancel");
        assert_eq!(cancelled.status, SchedulerJobStatus::Cancelled);
        assert_eq!(
            cancelled.cancellation_reason.as_deref(),
            Some("user requested")
        );
    }

    #[test]
    fn cancel_active_only_cancels_open_jobs() {
        let scheduler = Scheduler::new();
        let first = scheduler
            .schedule(SchedulerJobSpec {
                name: "first".to_string(),
                command: "do first".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("first");
        scheduler.cancel(first.id, "already done").expect("cancel");
        scheduler
            .schedule(SchedulerJobSpec {
                name: "second".to_string(),
                command: "do second".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("second");

        assert_eq!(scheduler.cancel_active("paused"), 1);
        assert_eq!(
            scheduler
                .list()
                .iter()
                .filter(|job| job.status == SchedulerJobStatus::Cancelled)
                .count(),
            2
        );
    }

    #[test]
    fn cancel_active_cancels_scheduled_and_running_jobs() {
        let scheduler = Scheduler::new();
        let scheduled = scheduler
            .schedule(SchedulerJobSpec {
                name: "scheduled".to_string(),
                command: "do scheduled".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("scheduled");
        let running = scheduler
            .schedule(SchedulerJobSpec {
                name: "running".to_string(),
                command: "do running".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("running");
        scheduler.mark_running(running.id).expect("mark running");

        assert_eq!(scheduler.cancel_active("emergency pause"), 2);

        let cancelled = scheduler.list();
        assert!(cancelled.iter().all(|job| {
            job.status == SchedulerJobStatus::Cancelled
                && job.cancelled_at.is_some()
                && job.cancellation_reason.as_deref() == Some("emergency pause")
        }));
        assert!(cancelled.iter().any(|job| job.id == scheduled.id));
        assert!(cancelled.iter().any(|job| job.id == running.id));
    }

    #[test]
    fn completes_and_fails_open_jobs() {
        let scheduler = Scheduler::new();
        let completed = scheduler
            .schedule(SchedulerJobSpec {
                name: "completed".to_string(),
                command: "record completion".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("completed");
        let failed = scheduler
            .schedule(SchedulerJobSpec {
                name: "failed".to_string(),
                command: "record failure".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("failed");

        scheduler.mark_running(completed.id).expect("mark running");
        let completed = scheduler.complete(completed.id).expect("complete");
        let failed = scheduler.fail(failed.id).expect("fail");

        assert_eq!(completed.status, SchedulerJobStatus::Completed);
        assert_eq!(failed.status, SchedulerJobStatus::Failed);
        assert!(scheduler.cancel(completed.id, "too late").is_err());
        assert!(scheduler.mark_running(failed.id).is_err());
    }

    #[test]
    fn detects_due_manual_once_and_interval_jobs() {
        let scheduler = Scheduler::new();
        let now = Utc::now();
        let manual = scheduler
            .schedule(SchedulerJobSpec {
                name: "manual".to_string(),
                command: "status".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("manual");
        let past_once = scheduler
            .schedule(SchedulerJobSpec {
                name: "past once".to_string(),
                command: "status".to_string(),
                trigger: TriggerKind::OnceAt {
                    run_at: now - Duration::seconds(1),
                },
            })
            .expect("past once");
        scheduler
            .schedule(SchedulerJobSpec {
                name: "future once".to_string(),
                command: "status".to_string(),
                trigger: TriggerKind::OnceAt {
                    run_at: now + Duration::seconds(60),
                },
            })
            .expect("future once");
        let interval = scheduler
            .schedule(SchedulerJobSpec {
                name: "interval".to_string(),
                command: "status".to_string(),
                trigger: TriggerKind::Interval { every_seconds: 30 },
            })
            .expect("interval");

        let due = scheduler.due_jobs(now + Duration::seconds(31), 10);
        assert!(due.iter().any(|job| job.id == manual.id));
        assert!(due.iter().any(|job| job.id == past_once.id));
        assert!(due.iter().any(|job| job.id == interval.id));
        assert_eq!(scheduler.due_jobs(now + Duration::seconds(31), 2).len(), 2);
    }

    #[test]
    fn interval_jobs_can_be_rescheduled_after_running() {
        let scheduler = Scheduler::new();
        let job = scheduler
            .schedule(SchedulerJobSpec {
                name: "interval".to_string(),
                command: "status".to_string(),
                trigger: TriggerKind::Interval { every_seconds: 60 },
            })
            .expect("interval");

        scheduler.mark_running(job.id).expect("running");
        let rescheduled = scheduler
            .reschedule_interval(job.id)
            .expect("reschedule interval");

        assert_eq!(rescheduled.status, SchedulerJobStatus::Scheduled);
        assert!(rescheduled.updated_at >= job.updated_at);
    }
}
