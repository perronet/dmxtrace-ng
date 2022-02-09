//! This crate contains the `rbf-trace`'s model extraction module.
//! 
//! # Example
//! Extract all the supported model from a trace.
//! 
//! ```
//! use rbftrace_core::{
//!     trace::{Trace, TraceEvent},
//!     sys_conf::SysConf,
//!     time::Time};
//! 
//! use rbftrace_model_extraction::{
//!     SystemModelExtractor,
//!     composite::{CompositeModelExtractor, CompositeExtractionParams}
//! };
//! 
//! let trace = Trace::from([
//!            TraceEvent::activation(0, Time::from_ms(5.)),
//!            TraceEvent::dispatch(0, Time::from_ms(5.)),
//!            TraceEvent::deactivation(0, Time::from_ms(7.)),
            
//!            TraceEvent::activation(0, Time::from_ms(15.)),
//!            TraceEvent::dispatch(0, Time::from_ms(15.)),
//!            TraceEvent::deactivation(0, Time::from_ms(18.)),
            
//!            TraceEvent::activation(0, Time::from_ms(25.)),
//!            TraceEvent::dispatch(0,Time::from_ms(25.)),
//!            TraceEvent::deactivation(0, Time::from_ms(26.))
//! ]);

//! let sysconf   = SysConf::default();
//! let params    = CompositeExtractionParams::default();
//! 
//! let mut extractor: SystemModelExtractor<CompositeModelExtractor>;
//! extractor = SystemModelExtractor::new(params, sysconf);
//!  
//! for event in trace.events() {
//!     extractor.push_event(*event);
//! }
//! 
//! let model = extractor.extract_model();
//! ``` 

use rbftrace_core::trace::{Trace, TraceEvent};
use rbftrace_core::model::{SystemModel};
use rbftrace_core::sys_conf::{SysConf, Pid};

use std::collections::HashMap;

pub mod periodic;
pub mod rbf;
pub mod job;
pub mod composite;

/// This trait defines the behaviour of a task level extractor.
/// A task level extractor extracts a model from a stream of trace 
/// events generated a single task, ie, all events have the same pid.
/// The multiplexing of events of different tasks is handled by 
/// SystemModelExtractors.
/// All expected task level model extractors are expected to implement this trait.
pub trait TaskModelExtractor {
    /// The type of Model being extracted by this extractor.
    type Model;

    /// The type of extraction parameters needed to build this extractor.
    type Params: Default;

    /// Build from exctraction parameters
    fn from_params(params: &Self::Params) -> Self;

    /// Indicates wether the current state allows to extract a valid model or not.
    fn is_matching(&self) -> bool;

    /// Update the model with a new event. 
    /// Returns a boolean value indicating wether the extracted model changes after update.
    fn push_event(&mut self, event: TraceEvent) -> bool;

    /// Extract a model based on the current extractor state if matching.
    fn extract_model(&self) -> Option<Self::Model>;

    /// Call `push_trace` and check if the model is still matching
    fn match_trace(&mut self, trace: &Trace) -> bool {
        self.push_trace(trace);
        self.is_matching()
    }

    /// Push all the event in a trace.
    /// Assume all the events have the same pid.
    /// If you want to perform a one shot matching, please use `SystemModelExtractor::from_trace` instead.
    fn push_trace(&mut self, trace: &Trace) {
        for event in trace.events() {
            self.push_event(*event);
        }
    }
}

/**
 * A System level extractor, in charge of multiplexing the events 
 * of different tasks, and to handle issues related to system configuration.
 */
pub struct SystemModelExtractor<T: TaskModelExtractor> {
    params: T::Params,
    sys_conf: SysConf,
    extractors: HashMap<Pid, T>,
}

impl<T: TaskModelExtractor> SystemModelExtractor<T> {
    pub fn new(params: T::Params, sys_conf: SysConf) -> Self {
        Self {
            params,
            sys_conf,
            extractors: HashMap::new()
        }
    }

    /// Push an event to the model extractor associated with the pid of this event's emitter.
    pub fn push_event(&mut self, event: TraceEvent) -> bool {
        let params = &self.params;
        self.extractors
            .entry(event.pid)
            .or_insert_with(|| T::from_params(params))
            .push_event(event)
    }

    /// Extract a system model from the current extraction state
    pub fn extract_model(&self) -> SystemModel<T::Model> {
        let mut system_model = SystemModel::new(self.sys_conf.clone());

        for (pid, extractor) in self.extractors.iter() {
            // m.set_rbf(*pid, extractor.extract_rbf());
            if let Some(task_model) = extractor.extract_model() {
                system_model.set_task_model(*pid, task_model);
            }
        }

        system_model
    }

    /// Build system level extractor, push all the event of a trace and return the extracted SystemModel.
    /// Use this method for one shot model extraction.
    pub fn extract_from_trace(params: T::Params, sys_conf: SysConf, trace: Trace) -> SystemModel<T::Model> {
        let mut extractor = Self::new(params, sys_conf);

        for event in trace.events() {
            extractor.push_event(*event);
        }

        extractor.extract_model()
    }
} 
