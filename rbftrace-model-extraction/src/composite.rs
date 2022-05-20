//! This module contains an extractor composed of all the supported type of extractors.
//! This is useful to extract several models at once.

use rbftrace_core::{model::PeriodicTask, model::PeriodicSelfSuspendingTask,
                    rbf::RbfCurve, trace::TraceEvent, time::Time};

use crate::{periodic::{PeriodicTaskExtractionParams, PeriodicTaskExtractor},
            spectral::{SpectralExtractionParams, SpectralExtractor},
            rbf::{RBFExtractor, RBFExtractionParams}, TaskModelExtractor};

pub struct CompositeModelExtractor {
    periodic_extractor: PeriodicTaskExtractor,
    spectral_extractor: SpectralExtractor,
    rbf_extractor: RBFExtractor,
    periodic_enabled: bool,
    spectral_enabled: bool,
    rbf_enabled: bool,
}

#[derive(Default)]
pub struct CompositeExtractionParams {
    pub periodic: PeriodicTaskExtractionParams,
    pub spectral: SpectralExtractionParams,
    pub rbf: RBFExtractionParams,
    pub periodic_enabled: bool,
    pub spectral_enabled: bool,
    pub rbf_enabled: bool,
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
            rbf_extractor: RBFExtractor::from_params(&params.rbf),
            periodic_enabled: params.periodic_enabled,
            spectral_enabled: params.spectral_enabled,
            rbf_enabled: params.rbf_enabled,
        }
    }

    fn is_matching(&self) -> bool {
        self.periodic_extractor.is_matching() || self.spectral_extractor.is_matching() || self.rbf_extractor.is_matching()
    }

    fn push_event(&mut self, event: TraceEvent) -> bool {
        let mut periodic_changed = false;
        let mut spectral_changed = false;
        let mut rbf_changed = false;

        if self.rbf_enabled {
            rbf_changed = self.rbf_extractor.push_event(event);
        }
        if self.periodic_enabled {
            periodic_changed = self.periodic_extractor.push_event(event);
        }
        if self.spectral_enabled {
            spectral_changed = self.spectral_extractor.push_event(event);
        }

        periodic_changed || spectral_changed || rbf_changed
    }

    /// Implements the hierarchy of the model extractors.
    fn extract_model(&mut self) -> Option<Self::Model> {
        let mut periodic = None;
        let mut periodic_ss = None;
        let mut rbf = RbfCurve::new(0, 1);

        if self.rbf_enabled {
            rbf = self.rbf_extractor.extract_model().unwrap(); // RBFs can always be extracted
        }
        if self.periodic_enabled {
            periodic = self.periodic_extractor.extract_model();
        }
        if self.spectral_enabled && periodic.is_none() {
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