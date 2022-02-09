//! This module contains an extractor composed of all the supported type of extractors.
//! This is useful to extract several models at once.

use rbftrace_core::{model::PeriodicTask, rbf::RbfCurve, trace::TraceEvent};

use crate::{periodic::{PeriodicTaskExtractionParams, PeriodicTaskExtractor}, rbf::{RBFExtractor, RBFExtractionParams}, TaskModelExtractor};

pub struct CompositeModelExtractor {
    periodic_extractor: PeriodicTaskExtractor,
    rbf_extractor: RBFExtractor,
}

#[derive(Default)]
pub struct CompositeExtractionParams {
    pub periodic: PeriodicTaskExtractionParams,
    pub rbf: RBFExtractionParams,
}

#[derive(PartialEq, Eq, Clone)]
pub struct CompositeModel {
    pub periodic: Option<PeriodicTask>,
    pub rbf: RbfCurve
}

impl CompositeModel {
    pub fn new(periodic: Option<PeriodicTask>, rbf: RbfCurve) -> Self {
        Self {periodic, rbf}
    }
}

impl TaskModelExtractor for CompositeModelExtractor {
    type Model = CompositeModel;
    type Params = CompositeExtractionParams;

    fn from_params(params: &Self::Params) -> Self {
        Self {
            periodic_extractor: PeriodicTaskExtractor::from_params(&params.periodic),
            rbf_extractor: RBFExtractor::from_params(&params.rbf) 
        }
    }

    fn is_matching(&self) -> bool {
        self.periodic_extractor.is_matching() || self.rbf_extractor.is_matching()
    }

    fn push_event(&mut self, event: TraceEvent) -> bool {
        let periodic_changed= self.periodic_extractor.push_event(event);
        let rbf_changed = self.rbf_extractor.push_event(event);

        periodic_changed || rbf_changed
    }

    fn extract_model(&self) -> Option<Self::Model> {
        let periodic = self.periodic_extractor.extract_model();
        let rbf = self.rbf_extractor.extract_model().unwrap();

        Some(CompositeModel::new(periodic, rbf))
    }

    fn match_trace(&mut self, trace: &rbftrace_core::trace::Trace) -> bool {
        self.push_trace(trace);
        self.is_matching()
    }

    fn push_trace(&mut self, trace: &rbftrace_core::trace::Trace) {
        for event in trace.events() {
            self.push_event(*event);
        }
    }
}