//! This module contains an extractor composed of all the supported type of extractors.
//! This is useful to extract several models at once.

use rbftrace_core::{model::PeriodicTask, model::PeriodicSelfSuspendingTask,
                    rbf::RbfCurve, trace::TraceEvent};

use crate::{periodic::{PeriodicTaskExtractionParams, PeriodicTaskExtractor},
            spectral::{SpectralExtractionParams, SpectralExtractor},
            rbf::{RBFExtractor, RBFExtractionParams}, TaskModelExtractor};

pub struct CompositeModelExtractor {
    periodic_extractor: PeriodicTaskExtractor,
    spectral_extractor: SpectralExtractor,
    rbf_extractor: RBFExtractor,
}

#[derive(Default)]
pub struct CompositeExtractionParams {
    pub periodic: PeriodicTaskExtractionParams,
    pub spectral: SpectralExtractionParams,
    pub rbf: RBFExtractionParams,
}

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct CompositeModel {
    pub periodic: Option<PeriodicTask>,
    pub periodic_ss: Option<PeriodicSelfSuspendingTask>,
    pub rbf: RbfCurve
}

impl CompositeModel {
    pub fn new(periodic: Option<PeriodicTask>, periodic_ss: Option<PeriodicSelfSuspendingTask>,
            rbf: RbfCurve) -> Self {
        Self {periodic, periodic_ss, rbf}
    }
}

impl TaskModelExtractor for CompositeModelExtractor {
    type Model = CompositeModel;
    type Params = CompositeExtractionParams;

    fn from_params(params: &Self::Params) -> Self {
        Self {
            periodic_extractor: PeriodicTaskExtractor::from_params(&params.periodic),
            spectral_extractor: SpectralExtractor::from_params(&params.spectral),
            rbf_extractor: RBFExtractor::from_params(&params.rbf) 
        }
    }

    fn is_matching(&self) -> bool {
        self.periodic_extractor.is_matching() || self.spectral_extractor.is_matching() || self.rbf_extractor.is_matching()
    }

    fn push_event(&mut self, event: TraceEvent) -> bool {
        let periodic_changed = self.periodic_extractor.push_event(event);
        let spectral_changed = self.spectral_extractor.push_event(event);
        let rbf_changed = self.rbf_extractor.push_event(event);

        periodic_changed || spectral_changed || rbf_changed
    }

    /// Implements the hierarchy of the model extractors.
    fn extract_model(&mut self) -> Option<Self::Model> {
        let periodic = self.periodic_extractor.extract_model();
        let rbf = self.rbf_extractor.extract_model().unwrap(); // RBFs are always extracted
        
        let mut periodic_ss = None;
        if periodic.is_none() {
            periodic_ss = self.spectral_extractor.extract_model();
        }

        Some(CompositeModel::new(periodic, periodic_ss, rbf))
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

// TODO unit tests for composite