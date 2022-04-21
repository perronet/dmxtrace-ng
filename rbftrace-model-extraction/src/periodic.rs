//! This modules contains a model extractor for non self-suspending periodic tasks.

use rbftrace_core::{
    trace::{TraceEvent, TraceEventType}, 
    time::Time, math::Interval, model::PeriodicTask};


use ringbuffer::{RingBuffer, AllocRingBuffer, RingBufferWrite, RingBufferExt};

use crate::{TaskModelExtractor, job::{JobExtractor}};
use rbftrace_core::model::Job;

pub struct PeriodicTaskExtractionParams {
    pub resolution: Time,
    pub j_max: Time
}

impl Default for PeriodicTaskExtractionParams {
    fn default() -> Self {
        Self { resolution: Time::from_ms(0.1), 
               j_max: Time::from_ms(1.0)
            }
    }
}

pub struct PeriodicTaskExtractor {
    resolution: Time,
    j_max: Time,

    activation_history: AllocRingBuffer<TraceEvent>, // Only Activation events
    still_periodic: bool,

    current_model: Option<PeriodicTask>,

    average_gap: Time,
    wcet: Time,

    curr_period_range: Option<Interval<Time>>,
    job_detector: JobExtractor,

    last_job: Option<Job>,
}

impl PeriodicTaskExtractor {
    pub fn new(j_max: Time, resolution: Time) -> Self {
        let history_size_target = (2 * (j_max.to_ns() / resolution.to_ns()) + 1 ) as usize;
        let history_size = history_size_target.next_power_of_two();
        let activation_history = AllocRingBuffer::with_capacity(history_size);
        
        Self {
            resolution,
            j_max,
            activation_history,
            still_periodic: false,
            average_gap: Time::zero(),
            curr_period_range: None, 
            current_model: None,
            job_detector: JobExtractor::new(),
            wcet: Time::zero(),
            last_job: None,
        }
    }

    fn update_period_range(&mut self) {
        let event_count = self.activation_history.len() - 1;

        if event_count > 0 {
            let err = self.j_max / event_count;
            let upper = self.average_gap + err;
            let lower: Time;
            if err < self.average_gap {
                lower = self.average_gap - err;
            } else {
                lower = Time::from(1);
            }
            
            let obs_period_range = Interval::closed(lower, upper);

            // Check if still periodic
            let new_period_range = self.curr_period_range.map_or(
                obs_period_range,
                |prev| prev.intersection(&obs_period_range));

            self.curr_period_range = Some(new_period_range);
        }
    }

    fn push_activation_and_update_average_gap(&mut self, event: TraceEvent) {
        let new_diff = event.instant - self.activation_history.back().unwrap().instant;

        let mut k = self.activation_history.len() - 1;

        let mut new_average_gap = self.average_gap;
        // update moving average
        if self.activation_history.is_full() {
            // update moving average
            let oldest_diff = self.activation_history.get(1).unwrap().instant - self.activation_history.get(0).unwrap().instant;
            new_average_gap += new_diff / k;
            new_average_gap -= oldest_diff / k;

        } else {
            // update cumulative moving average
            k += 1;
            new_average_gap += new_diff / k;
            new_average_gap -= self.average_gap / k;
        }

        self.activation_history.push(event);
        self.average_gap = new_average_gap;
    }

    fn find_period(&mut self) {
        if let Some(mut model) = self.current_model {
            let mut period = self.average_gap;
            let mut period_found = false;
            let interval_left = self.curr_period_range.unwrap().get_lower().unwrap();
            let interval_right = self.curr_period_range.unwrap().get_upper().unwrap();

            let min_magnitude = (self.resolution.to_ns() as f64).log10() as u32;
            let mut magnitude = 10;
            // Try down to minimal magnitude
            while !period_found && magnitude >= min_magnitude {
                let granularity = Time::from(10_u64.pow(magnitude));
                period = self.average_gap.round(granularity);
                if interval_left <= period && period <= interval_right {
                    period_found = true;
                }

                magnitude -= 1;
            }

            if period_found {
                model.period = period;
            } else {
                // No period found in the interval with granularity >= resolution
                // Pick a period anyway
                model.period = self.average_gap.round(self.resolution);
            }

            assert!(model.period >= Time::from(1));
            self.current_model.replace(model);
        }
    }

    fn extract_offset_and_jitter(&mut self) {
        if let Some(mut model) = self.current_model {
            let last_activation = self.activation_history.back().unwrap();
            let last_activation_jo = last_activation.instant % model.period;

            let mut min_jo = last_activation_jo;
            let mut max_jo = last_activation_jo;

            for event in self.activation_history.iter() {
                let jo = event.instant % model.period;

                if jo < min_jo {
                    min_jo = jo;
                }
                if jo > max_jo {
                    max_jo = jo
                }
            }

            model.offset = min_jo.truncate(self.resolution);
            model.jitter = max_jo - model.offset;
            
            self.current_model.replace(model);
        }

    }

    fn update_still_periodic(&mut self) {
        self.still_periodic = self.curr_period_range
                                  .map_or(false, |i| !i.is_empty());
        
       self.current_model = if self.still_periodic {
            Some(PeriodicTask::default())
        } else {
            None
        };
    }
    
    fn push_activation(&mut self, event: TraceEvent) {       
        assert!(event.is_activation());

        if self.activation_history.is_empty() {
            self.activation_history.push(event);

            return
        }

        self.push_activation_and_update_average_gap(event);

        self.update_period_range();

        self.update_still_periodic();
        
        if self.still_periodic {
            self.find_period();
            self.extract_offset_and_jitter();
        }
    }

    fn push_deactivation(&mut self, event: TraceEvent) {
        assert!(event.is_deactivation());
        
        if let Some(job) = &self.last_job {
            if job.completed_at == event.instant {
                self.wcet = self.wcet.max(job.execution_time);

                if let Some(mut model) = self.current_model {
                    model.wcet = self.wcet;
                    
                    self.current_model.replace(model);
                }
            }
        }
    }
}

impl TaskModelExtractor for PeriodicTaskExtractor {
    type Model = PeriodicTask;
    type Params = PeriodicTaskExtractionParams;

    
    fn from_params(params: &Self::Params) -> Self {
        Self::new(params.j_max, params.resolution)
    }

    fn is_matching(&self) -> bool {
        self.still_periodic
    }

    /// Returns true if the model could have changed.
    fn push_event(&mut self, event: TraceEvent) -> bool {
        let maybe_job = self.job_detector.push_event(&event);

        if maybe_job.is_some() {
            self.last_job = maybe_job;
        }

        match event.etype {
            TraceEventType::Activation => self.push_activation(event),
            TraceEventType::Deactivation => self.push_deactivation(event),
            _ => {}
        }

        maybe_job.is_some()
    }

    // The periodic extractor is completely incremental, so there is no need to manually trigger the extraction
    fn extract_model(&mut self) -> Option<Self::Model> {
        self.current_model
    }
}

// Reminder: These tests are using a Jmax of 1ms
#[cfg(test)]
mod test {
    use rbftrace_core::{time::Time, trace::{Trace, TraceEvent}, model::PeriodicTask};

    use crate::periodic::{PeriodicTaskExtractor, TaskModelExtractor};

    #[test]
    pub fn periodic_fixed_exec_time(){
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

        let mut extractor = PeriodicTaskExtractor::new(Time::from_ms(1.0), Time::from_ms(1.0));

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = PeriodicTask{
            period: Time::from_ms(10.0),
            jitter: Time::from_ms(0.0),
            offset: Time::from_ms(5.0),
            wcet: Time::from_ms(2.0)
        };

        assert_eq!(
            extractor.extract_model(),
            Some(expected)
        )
    }

    #[test]
    pub fn periodic(){
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

        let mut extractor = PeriodicTaskExtractor::new(Time::from_ms(1.0), Time::from_ms(1.0));

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = PeriodicTask{
            period: Time::from_ms(10.0),
            jitter: Time::from_ms(0.0),
            offset: Time::from_ms(5.0),
            wcet: Time::from_ms(3.0)
        };

        assert_eq!(
            extractor.extract_model(),
            Some(expected)
        )
    }
    
    #[test]
    pub fn periodic_with_jitter(){
        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_ms(5.5)),
            TraceEvent::dispatch(0, Time::from_ms(5.5)),
            TraceEvent::deactivation(0, Time::from_ms(7.5)),

            
            TraceEvent::activation(0, Time::from_ms(15.3)),
            TraceEvent::dispatch(0, Time::from_ms(15.3)),
            TraceEvent::deactivation(0, Time::from_ms(18.3)),

            
            TraceEvent::activation(0, Time::from_ms(25.0)),
            TraceEvent::dispatch(0,Time::from_ms(25.0)),
            TraceEvent::deactivation(0, Time::from_ms(26.0)),
            
            TraceEvent::activation(0, Time::from_ms(35.5)),
            TraceEvent::dispatch(0, Time::from_ms(35.5)),
            TraceEvent::deactivation(0, Time::from_ms(37.5)),

            
            TraceEvent::activation(0, Time::from_ms(45.3)),
            TraceEvent::dispatch(0, Time::from_ms(45.3)),
            TraceEvent::deactivation(0, Time::from_ms(48.3)),

            
            TraceEvent::activation(0, Time::from_ms(55.0) ),
            TraceEvent::dispatch(0,Time::from_ms(55.0) ),
            TraceEvent::deactivation(0, Time::from_ms(56.0)),
            
            TraceEvent::activation(0, Time::from_ms(65.5)),
            TraceEvent::dispatch(0, Time::from_ms(65.5)),
            TraceEvent::deactivation(0, Time::from_ms(67.5)),

            
            TraceEvent::activation(0, Time::from_ms(75.3)),
            TraceEvent::dispatch(0, Time::from_ms(75.3)),
            TraceEvent::deactivation(0, Time::from_ms(78.3)),

            
            TraceEvent::activation(0, Time::from_ms(85.0) ),
            TraceEvent::dispatch(0,Time::from_ms(85.0) ),
            TraceEvent::deactivation(0, Time::from_ms(86.0)),
            
            TraceEvent::activation(0, Time::from_ms(95.0) ),
            TraceEvent::dispatch(0,Time::from_ms(95.0) ),
            TraceEvent::deactivation(0, Time::from_ms(96.0)),
            
            TraceEvent::activation(0, Time::from_ms(105.0) ),
            TraceEvent::dispatch(0,Time::from_ms(105.0) ),
            TraceEvent::deactivation(0, Time::from_ms(106.0))
        ]);

        let mut extractor = PeriodicTaskExtractor::new(Time::from_ms(1.0), Time::from_ms(0.1));

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = PeriodicTask{
            period: Time::from_ms(10.0),
            jitter: Time::from_ms(0.5),
            offset: Time::from_ms(5.0),
            wcet: Time::from_ms(3.0)
        };

        assert_eq!(
            extractor.extract_model(),
            Some(expected)
        )
    }

    #[test]
    pub fn periodic_with_jitter_2(){
        let trace = Trace::from([
            TraceEvent::activation(0, Time::from_ms(5.5)),
            TraceEvent::dispatch(0, Time::from_ms(5.5)),
            TraceEvent::deactivation(0, Time::from_ms(7.5)),

            
            TraceEvent::activation(0, Time::from_ms(15.5)),
            TraceEvent::dispatch(0, Time::from_ms(15.5)),
            TraceEvent::deactivation(0, Time::from_ms(18.3)),

            
            TraceEvent::activation(0, Time::from_ms(25.0)),
            TraceEvent::dispatch(0,Time::from_ms(25.0)),
            TraceEvent::deactivation(0, Time::from_ms(26.0)),
            
            TraceEvent::activation(0, Time::from_ms(35.5)),
            TraceEvent::dispatch(0, Time::from_ms(35.5)),
            TraceEvent::deactivation(0, Time::from_ms(37.5)),

            
            TraceEvent::activation(0, Time::from_ms(45.3)),
            TraceEvent::dispatch(0, Time::from_ms(45.3)),
            TraceEvent::deactivation(0, Time::from_ms(48.3)),

            
            TraceEvent::activation(0, Time::from_ms(55.0) ),
            TraceEvent::dispatch(0,Time::from_ms(55.0) ),
            TraceEvent::deactivation(0, Time::from_ms(56.0)),
            
            TraceEvent::activation(0, Time::from_ms(65.5)),
            TraceEvent::dispatch(0, Time::from_ms(65.5)),
            TraceEvent::deactivation(0, Time::from_ms(67.5)),

            
            TraceEvent::activation(0, Time::from_ms(75.3)),
            TraceEvent::dispatch(0, Time::from_ms(75.3)),
            TraceEvent::deactivation(0, Time::from_ms(78.3)),

            
            TraceEvent::activation(0, Time::from_ms(85.1) ),
            TraceEvent::dispatch(0,Time::from_ms(85.1) ),
            TraceEvent::deactivation(0, Time::from_ms(86.0)),
            
            TraceEvent::activation(0, Time::from_ms(95.2) ),
            TraceEvent::dispatch(0,Time::from_ms(95.2) ),
            TraceEvent::deactivation(0, Time::from_ms(96.0)),
            
            TraceEvent::activation(0, Time::from_ms(105.4) ),
            TraceEvent::dispatch(0,Time::from_ms(105.4) ),
            TraceEvent::deactivation(0, Time::from_ms(106.0))
        ]);

        let mut extractor = PeriodicTaskExtractor::new(Time::from_ms(1.0), Time::from_ms(0.1));

        for event in trace.events() {
            extractor.push_event(*event);
        }

        let expected = PeriodicTask{
            period: Time::from_ms(10.0),
            jitter: Time::from_ms(0.5),
            offset: Time::from_ms(5.0),
            wcet: Time::from_ms(3.0)
        };

        assert_eq!(
            extractor.extract_model(),
            Some(expected)
        )
    }
    
    #[test]
    pub fn fail_on_sporadic(){
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

        let mut extractor = PeriodicTaskExtractor::new(Time::from_ms(0.5), Time::from_ms(0.1));
        
        for event in trace.events() {
            extractor.push_event(*event);
        }

        assert!(!extractor.is_matching());
            
        assert_eq!(
            extractor.extract_model(), None
        )
    }
}