//! This module contains an RBF extractor.

use rbftrace_core::rbf::RbfCurve;

use crate::{TaskModelExtractor, job::JobExtractor};

pub struct RBFExtractionParams {
    pub window_size: usize
}

impl Default for RBFExtractionParams {
    fn default() -> Self {
        Self { window_size: 1000 
        }
    }
}

pub struct RBFExtractor {
    job_detector: JobExtractor,
    rbf: RbfCurve
}

impl TaskModelExtractor for RBFExtractor {
    type Model = RbfCurve;
    type Params = RBFExtractionParams;

    fn from_params(params: &Self::Params) -> Self {
        Self::new(params.window_size)
    }

    fn is_matching(&self) -> bool {
        true
    }

    fn push_event(&mut self, event: rbftrace_core::trace::TraceEvent) -> bool {
        let maybe_job = self.job_detector.push_event(&event);

        if let Some(job) = &maybe_job {
            self.rbf.add_arrival(job.arrived_at, job.execution_time);
        }

        maybe_job.is_some()
    }

    fn extract_model(&self) -> Option<Self::Model> {
        Some(self.rbf.clone())
    }
}

impl RBFExtractor {
    fn new(window_size: usize) -> Self {
        Self {
            job_detector: JobExtractor::new(),
            rbf: RbfCurve::new(0, window_size)
        }
    }
}