//! This module contains a job extractor. 

use rbftrace_core::{trace::TraceEvent, time::Time, model::Job};

pub struct JobExtractor {
    last_event: Option<TraceEvent>,
    last_activation: Option<TraceEvent>,
    preemption_time: Time,
}

impl JobExtractor {
    pub fn new () -> Self {
        Self {
            last_event: None,
            last_activation: None,
            preemption_time: Time::zero(),
        }
    }

    /// `push_event` updates the internal state with an event 
    /// and returns `Some(job)` if this event marks the completion of a job.
    /// Returns None if the arrival of the job has been pushed in the `JobExtractor`.
    pub fn push_event(&mut self, event: &TraceEvent) -> Option<Job>{
        if event.is_activation() {
            self.preemption_time = Time::zero();
            self.last_activation = Some(*event);
        }

        if event.is_deactivation() {
            if let Some(last_activation) = self.last_activation {
                assert!(last_activation.instant <= event.instant);
                self.last_event = Some(*event);

                return Some(Job {
                    execution_time: event.instant - last_activation.instant - self.preemption_time,
                    arrived_at: last_activation.instant,
                    completed_at: event.instant,
                    preemption_time: self.preemption_time,
                });
            } 
        }

        if event.is_dispatch() {
            if let Some(last_event) = self.last_event {
                if last_event.is_preemption() {
                    assert!(last_event.instant <= event.instant);
                    self.preemption_time = event.instant - last_event.instant;
                }
            }
        }
        
        self.last_event = Some(*event);
        
        None
    }

    /// Indicates if the last events pushed in the extractor marked a job complection.
    pub fn last_event_was_job_completion(&self) -> bool {
        self.last_event
            .map_or(false, |e| e.is_deactivation())
    }
}