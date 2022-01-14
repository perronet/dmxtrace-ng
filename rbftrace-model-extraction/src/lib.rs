use rbftrace_core::rbf::RbfCurve;
use rbftrace_core::trace::{Trace, TraceEvent};
use rbftrace_core::model::{SystemModel, ScalarTaskModel};
use rbftrace_core::sys_conf::{SysConf};
use rbftrace_core::time::{Time};

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
            j_max: Time::from_ms(1.0), 
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
            acycle: InvocationCycle::new(pid, IcHeuristic::Suspension, Time::zero()), 
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

        false
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
            sys_conf,
            extractors: HashMap::new(),
        }
    }

    /// Returns true if the model could have changed
    pub fn push_event(&mut self, event: &TraceEvent) -> bool {
        let params = self.extraction_params;
        let sys_conf = self.sys_conf.clone();
        self.extractors
            .entry(event.pid)
            .or_insert_with(|| {
              IncrementalTaskModelExtractor::new(params, sys_conf, event.pid)  
            })
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
        time::{Time},
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

        assert!(model.pids().next().is_none());
    }

    #[test]
    pub fn periodic_fixed_exec_time(){
        let mut params = ModelExtractionParameters::default();
        let sys_conf = SysConf::default();
        params.set_jmax(Time::from_ms(1.0) );


        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_ms(5.0) ),

            TraceEvent::dispatch(0, Time::from_ms(5.0) ),

            TraceEvent::deactivation(0, Time::from_ms(7.0) ),

            
            TraceEvent::activation(0, Time::from_ms(15.0) ),

            TraceEvent::dispatch(0, Time::from_ms(15.0) ),

            TraceEvent::deactivation(0, Time::from_ms(17.0) ),

            
            TraceEvent::activation(0, Time::from_ms(25.0) ),

            TraceEvent::dispatch(0,Time::from_ms(25.0) ),

            TraceEvent::deactivation(0, Time::from_ms(27.0) )

        ]);

        let mut extractor = IncrementalTaskModelExtractor::new(params, sys_conf, 0);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = ScalarTaskModel {
            arrival_model: JobArrivalModel::PeriodicJitterOffset{
                period: Time::from_ms(10.0) ,

                jitter: Time::from_ms(0.0) ,

                offset: Time::from_ms(5.0) 

            },
            
            execution_time: Time::from_ms(2.0) 

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
        params.set_jmax(Time::from_ms(1.0) );


        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_ms(5.)),
            TraceEvent::dispatch(0, Time::from_ms(5.)),
            TraceEvent::deactivation(0, Time::from_ms(7.)),
            
            TraceEvent::activation(0, Time::from_ms(15.)),
            TraceEvent::dispatch(0, Time::from_ms(15.)),
            TraceEvent::deactivation(0, Time::from_ms(18.)),
            
            TraceEvent::activation(0, Time::from_ms(25.)),
            TraceEvent::dispatch(0,Time::from_ms(25.)),
            TraceEvent::deactivation(0, Time::from_ms(26.))
        ]);

        let mut extractor = IncrementalTaskModelExtractor::new(params, sys_conf, 0);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = ScalarTaskModel {
            arrival_model: JobArrivalModel::PeriodicJitterOffset{
                period: Time::from_ms(10.0) ,

                jitter: Time::from_ms(0.0) ,

                offset: Time::from_ms(5.0) 

            },
            
            execution_time: Time::from_ms(3.0) 

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
        params.set_jmax(Time::from_ms(1.0) );


        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_ms(5.5) ),

            TraceEvent::dispatch(0, Time::from_ms(5.5) ),

            TraceEvent::deactivation(0, Time::from_ms(7.5) ),

            
            TraceEvent::activation(0, Time::from_ms(15.3) ),

            TraceEvent::dispatch(0, Time::from_ms(15.3) ),

            TraceEvent::deactivation(0, Time::from_ms(18.3) ),

            
            TraceEvent::activation(0, Time::from_ms(25.0) ),

            TraceEvent::dispatch(0,Time::from_ms(25.0) ),

            TraceEvent::deactivation(0, Time::from_ms(26.0) )

        ]);

        let mut extractor = IncrementalTaskModelExtractor::new(params, sys_conf, 0);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = ScalarTaskModel {
            arrival_model: JobArrivalModel::PeriodicJitterOffset{
                period: Time::from_ms(10.0) ,

                jitter: Time::from_ms(500.0) ,

                offset: Time::from_ms(5.0) 

            },
            
            execution_time: Time::from_ms(3.0) 

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
        params.set_jmax(Time::from_us(500.0) );


        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_ms(2.0) ),
            TraceEvent::dispatch(0, Time::from_ms(2.0) ),
            TraceEvent::deactivation(0, Time::from_ms(2.1) ),

            
            TraceEvent::activation(0, Time::from_ms(5.0) ),
            TraceEvent::dispatch(0, Time::from_ms(5.0) ),
            TraceEvent::deactivation(0, Time::from_ms(5.1) ),

            
            TraceEvent::activation(0, Time::from_ms(6.0)),
            TraceEvent::dispatch(0, Time::from_ms(6.0)),
            TraceEvent::deactivation(0, Time::from_ms(6.1)),

            
            TraceEvent::activation(0, Time::from_ms(7.0)),
            TraceEvent::dispatch(0, Time::from_ms(7.0)),
            TraceEvent::deactivation(0, Time::from_ms(7.1)),

            
            TraceEvent::activation(0, Time::from_ms(9.0)),
            TraceEvent::dispatch(0, Time::from_ms(9.0)),
            TraceEvent::deactivation(0, Time::from_ms(9.1)),


        ]);
        
        Time::from_ms(1.0).to_ns();

        let mut extractor = IncrementalTaskModelExtractor::new(params, sys_conf, 0);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = ScalarTaskModel::sporadic(Time::from_us(100.0), Time::from_ms(1.));

        assert_eq!(
            extractor.extract_scalar_model(),
            Some(expected)
        )
    }
}