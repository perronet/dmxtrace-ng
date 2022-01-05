use rbftrace_core::rbf::RbfCurve;
use rbftrace_core::trace::{Trace, TraceEvent};
use rbftrace_core::model::{SystemModel, ScalarTaskModel};
use rbftrace_core::sys_conf::{SysConf};
use rbftrace_core::time::{Time, ONE_MS};

mod mm_scalar;
mod arrival;
mod period_selection;

use mm_scalar::ScalarMM;

use arrival::{
    invocation_cycle::{InvocationCycle, IcHeuristic}
};

use rbftrace_core::time::{Pid};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct ModelExtractionParameters {
    j_max: Time,
    rbf_window_size: usize
}

impl Default for ModelExtractionParameters {
    fn default() -> Self {
        Self { 
            j_max: 1 * ONE_MS, 
            rbf_window_size: 1000
        }
    }
}

impl ModelExtractionParameters {
    pub fn set_jmax(&mut self, j_max: Time) {
        self.j_max = j_max;
    }

    pub fn get_jmax(&self) -> Time {
        self.j_max
    }

    pub fn set_rbf_window_size(&mut self, size: usize) {
        self.rbf_window_size = size;
    }

    pub fn get_rbf_window_size(&self) -> usize {
        self.rbf_window_size
    }
}
pub struct IncrementalTaskModelExtractor {
    pid: Pid,
    mm: ScalarMM,
    acycle: InvocationCycle,
    current_rbf: RbfCurve,
    arrival_cnt: u64,
}

impl IncrementalTaskModelExtractor {
    pub fn new(params: ModelExtractionParameters, _sys_conf: SysConf, pid: Pid) -> Self {
        IncrementalTaskModelExtractor { 
            pid,
            mm: ScalarMM::new(params.get_jmax(), 1000), // TODO the buf_size should be inside ModelExtractionParameters
            acycle: InvocationCycle::new(pid, IcHeuristic::Suspension, 0), 
            current_rbf:RbfCurve::new(pid, params.get_rbf_window_size()), 
            arrival_cnt: 0,
        }
    }

    /// Returns true if the model could have changed.
    pub fn push_event(&mut self, event: TraceEvent) -> bool {
        if let Some(arrival) = self.acycle.update_activation_cycle(event) {
            self.current_rbf.add_arrival(arrival.instant, arrival.cost);
            self.mm.update_internal_state(self.pid, arrival);
            self.arrival_cnt += 1;

            // It's not possible to match a model with only 1 arrival
            if self.arrival_cnt >= 2 {
                return true
            }
        }

        return false
    }

    /// Returns None if there are less than 2 arrivals.
    /// In which case no model can be matched.
    pub fn extract_scalar_model(&self) -> Option<ScalarTaskModel> {
        self.mm.extract_model(self.pid)
    }

    pub fn extract_rbf(&self) -> RbfCurve {
        self.current_rbf.clone()
    }
}

pub struct IncrementalSystemModelExtractor {
    sys_conf: SysConf,
    extraction_params: ModelExtractionParameters,
    extractors: HashMap<Pid, IncrementalTaskModelExtractor>,
}

impl IncrementalSystemModelExtractor {
    pub fn new(params: ModelExtractionParameters, sys_conf: SysConf) -> IncrementalSystemModelExtractor {
        IncrementalSystemModelExtractor {
            extraction_params: params,
            sys_conf: sys_conf,
            extractors: HashMap::new(),
        }
    }

    /// Returns true if the model could have changed
    pub fn push_event(&mut self, event: &TraceEvent) -> bool {
        self.extractors
            .entry(event.pid)
            .or_insert(IncrementalTaskModelExtractor::new(self.extraction_params.clone(), self.sys_conf.clone(), event.pid))
            .push_event(*event)
    }

    pub fn extract_model(&self) -> SystemModel {
        let mut m = SystemModel::new(self.sys_conf.clone());

        for (pid, extractor) in self.extractors.iter() {
            m.set_rbf(*pid, extractor.extract_rbf());
            if let Some(model_pid) = extractor.extract_scalar_model() {
                m.set_scalar_model(*pid, model_pid);
            }
        }

        m
    }
}

pub struct ModelExtractor {
    params: ModelExtractionParameters,
    sys_conf: SysConf
}

impl ModelExtractor {
    pub fn new(params: ModelExtractionParameters, sys_conf: SysConf) -> Self {
        ModelExtractor {
            params,
            sys_conf
        }
    }

    pub fn extract_model(&self, trace: Trace) -> SystemModel {
        let mut state = IncrementalSystemModelExtractor::new(
            self.params,
            self.sys_conf.clone());

        for event in trace.events() {
            state.push_event(event);
        }

        state.extract_model()
    }
}

#[cfg(test)]
mod tests {
    use rbftrace_core::{
        sys_conf::{SysConf},
        time::{Pid, ONE_MS, ONE_US},
        trace::{Trace, TraceEvent}, model::{ScalarTaskModel, JobArrivalModel}
    };

    use crate::{ModelExtractor, ModelExtractionParameters, IncrementalTaskModelExtractor};

    #[test]
    pub fn empty_trace() {
        let sys_conf = SysConf::default();
        let params = ModelExtractionParameters::default();
        let trace = Trace::from([]);

        let extractor = ModelExtractor::new(params, sys_conf);
        let model = extractor.extract_model(trace);

        let pids = model.pids().collect::<Vec<&Pid>>();

        assert!(pids.is_empty());
    }

    #[test]
    pub fn periodic_fixed_exec_time(){
        let mut params = ModelExtractionParameters::default();
        let sys_conf = SysConf::default();
        params.set_jmax(1 * ONE_MS);

        let trace = Trace::from([
            TraceEvent::activation(0, 5 * ONE_MS),
            TraceEvent::dispatch(0, 5 * ONE_MS),
            TraceEvent::deactivation(0, 7 * ONE_MS),
            
            TraceEvent::activation(0, 15 * ONE_MS),
            TraceEvent::dispatch(0, 15 * ONE_MS),
            TraceEvent::deactivation(0, 17 * ONE_MS),
            
            TraceEvent::activation(0, 25 * ONE_MS),
            TraceEvent::dispatch(0,25 * ONE_MS),
            TraceEvent::deactivation(0, 27 * ONE_MS)
        ]);

        let mut extractor = IncrementalTaskModelExtractor::new(params, sys_conf, 0);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = ScalarTaskModel {
            arrival_model: JobArrivalModel::PeriodicJitterOffset{
                period: 10 * ONE_MS,
                jitter: 0 * ONE_MS,
                offset: 5 * ONE_MS
            },
            
            execution_time: 2 * ONE_MS
        };

        assert_eq!(
            extractor.extract_scalar_model(),
            Some(expected)
        )
    }

    #[test]
    pub fn periodic(){
        let mut params = ModelExtractionParameters::default();
        let sys_conf = SysConf::default();
        params.set_jmax(1 * ONE_MS);

        let trace = Trace::from([
            TraceEvent::activation(0, 5 * ONE_MS),
            TraceEvent::dispatch(0, 5 * ONE_MS),
            TraceEvent::deactivation(0, 7 * ONE_MS),
            
            TraceEvent::activation(0, 15 * ONE_MS),
            TraceEvent::dispatch(0, 15 * ONE_MS),
            TraceEvent::deactivation(0, 18 * ONE_MS),
            
            TraceEvent::activation(0, 25 * ONE_MS),
            TraceEvent::dispatch(0,25 * ONE_MS),
            TraceEvent::deactivation(0, 26 * ONE_MS)
        ]);

        let mut extractor = IncrementalTaskModelExtractor::new(params, sys_conf, 0);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = ScalarTaskModel {
            arrival_model: JobArrivalModel::PeriodicJitterOffset{
                period: 10 * ONE_MS,
                jitter: 0 * ONE_MS,
                offset: 5 * ONE_MS
            },
            
            execution_time: 3 * ONE_MS
        };

        assert_eq!(
            extractor.extract_scalar_model(),
            Some(expected)
        )
    }
    
    // TODO Fix this test
    /// It takes more samples to converge when there is jitter in the trace.
    #[test]
    pub fn periodic_with_jitter(){
        let mut params = ModelExtractionParameters::default();
        let sys_conf = SysConf::default();
        params.set_jmax(1 * ONE_MS);

        let trace = Trace::from([
            TraceEvent::activation(0, 5_500 * ONE_US),
            TraceEvent::dispatch(0, 5_500 * ONE_US),
            TraceEvent::deactivation(0, 7_500 * ONE_US),
            
            TraceEvent::activation(0, 15_300 * ONE_US),
            TraceEvent::dispatch(0, 15_300 * ONE_US),
            TraceEvent::deactivation(0, 18_300 * ONE_US),
            
            TraceEvent::activation(0, 25_000 * ONE_US),
            TraceEvent::dispatch(0,25_000 * ONE_US),
            TraceEvent::deactivation(0, 26_000 * ONE_US)
        ]);

        let mut extractor = IncrementalTaskModelExtractor::new(params, sys_conf, 0);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = ScalarTaskModel {
            arrival_model: JobArrivalModel::PeriodicJitterOffset{
                period: 10 * ONE_MS,
                jitter: 500 * ONE_US,
                offset: 5 * ONE_MS
            },
            
            execution_time: 3 * ONE_MS
        };

        assert_eq!(
            extractor.extract_scalar_model(),
            Some(expected)
        )
    }
    
    #[test]
    pub fn sporadic(){
        let mut params = ModelExtractionParameters::default();
        let sys_conf = SysConf::default();
        params.set_jmax(500 * ONE_US);

        let trace = Trace::from([
            TraceEvent::activation(0, 2_000 * ONE_US),
            TraceEvent::dispatch(0, 2_000 * ONE_US),
            TraceEvent::deactivation(0, 2_100 * ONE_US),
            
            TraceEvent::activation(0, 5_000 * ONE_US),
            TraceEvent::dispatch(0, 5_000 * ONE_US),
            TraceEvent::deactivation(0, 5_100 * ONE_US),
            
            TraceEvent::activation(0, 6_000 * ONE_US),
            TraceEvent::dispatch(0, 6_000 * ONE_US),
            TraceEvent::deactivation(0, 6_100 * ONE_US),
            
            TraceEvent::activation(0, 7_000 * ONE_US),
            TraceEvent::dispatch(0, 7_000 * ONE_US),
            TraceEvent::deactivation(0, 7_100 * ONE_US),
            
            TraceEvent::activation(0, 9_000 * ONE_US),
            TraceEvent::dispatch(0, 9_000 * ONE_US),
            TraceEvent::deactivation(0, 9_100 * ONE_US),
        ]);

        let mut extractor = IncrementalTaskModelExtractor::new(params, sys_conf, 0);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = ScalarTaskModel::sporadic(100 * ONE_US, 1 * ONE_MS);

        assert_eq!(
            extractor.extract_scalar_model(),
            Some(expected)
        )
    }
}