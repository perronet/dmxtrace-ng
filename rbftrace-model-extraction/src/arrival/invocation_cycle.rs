use crate::arrival::arr::Arrival;
use rbftrace_core::time::*;
use rbftrace_core::trace::{
    TraceEventType,
    TraceEvent,
};

/*  The purpose of the invocation cycle is to transform raw events in "Arrival" events
    that can be understood by the model matcher. Raw events are fed in a state machine
    that emits Arrival events upon certain transitions.
*/

/*  When an invocation cycle completes, we register a valid arrival with a certain cost.
    What represents an invocation cycle (and the cost) depends on the picked heuristic.
*/

/*  HEURISTIC 1: Suspension
    An invocation cycle completes when a valid sequences of transitions 
    lead from an activation to a deactivation or exit.
*/

/*  HEURISTIC 2: SuspensionTimeout
    An invocation cycle completes when a valid sequences of transitions 
    lead from an activation A1 to a deactivation D1 such that the next activation A2
    is at least IC_TIMEOUT time units apart from D1.
    An exit event always counts as the end of a cycle.
*/

// Handles the activation state machine for a single process.
// Its purpose is to transform raw events in "Arrival" events that can be
// understood by the model matcher.
pub struct InvocationCycle {
    pub pid : Pid,
    pub activation : Time,
    pub last_event_type: Option<TraceEventType>,
    pub last_event_time : Time,
    pub curr_cost: Cost,    // Total cost (including self-suspension time)
    pub curr_ss_time: Cost, // Self-suspension time
    pub curr_ss_cnt: Cost,

    pub heuristic: IcHeuristic,
    pub timeout: Time, // Only used if heuristic is SuspensionTimeout
}

pub enum IcHeuristic {
    Suspension,
    SuspensionTimeout,
}

impl IcHeuristic {
    pub fn equals(&self, m: IcHeuristic) -> bool {
        return std::mem::discriminant(self) == std::mem::discriminant(&m);
    }
}

impl InvocationCycle {
    pub fn new(pid: Pid, heuristic: IcHeuristic, timeout: Time) -> InvocationCycle {
        InvocationCycle {
            pid : pid,
            activation : 0,
            last_event_type : None,
            last_event_time : 0,
            curr_cost : 0,
            curr_ss_time : 0,
            curr_ss_cnt : 0,
            heuristic : heuristic,
            timeout : timeout,
        }
    }

    pub fn reset(&mut self) {
        self.activation = 0;
        self.last_event_type = None;
        self.last_event_time = 0;
        self.curr_cost = 0;
        self.curr_ss_time = 0;
        self.curr_ss_cnt = 0;
    }

    // Only a Deactivation, Activation, or Exit can mark the completion of an invocation cycle
    pub fn update_activation_cycle(&mut self, event: TraceEvent) -> Option<Arrival> {
        match event.etype {
            TraceEventType::Activation =>   { self.activation(event.instant) },
            TraceEventType::Deactivation => { self.deactivation(event.instant) },
            TraceEventType::Preemption =>   { self.preemption(event.instant); None },
            TraceEventType::Dispatch =>       { self.resume(event.instant); None },
            TraceEventType::Exit =>         { self.exit(event.instant) },
        }
    }

    fn activation(&mut self, instant: Time) -> Option<Arrival> {
        if self.last_event_type.is_some() {
            assert!(instant > self.last_event_time);
        }
        
        match self.last_event_type
        {
            Some (TraceEventType::Deactivation) => {
                assert!(self.heuristic.equals(IcHeuristic::SuspensionTimeout));
                let time_since_last_deactivation = instant - self.last_event_time;

                if time_since_last_deactivation > self.timeout { // End cycle
                    let activation = self.activation;
                    let final_cost = self.curr_cost;
                    let final_ss_time = self.curr_ss_time;
                    let final_ss_cnt = self.curr_ss_cnt;
                    self.reset();
                    self.activation = instant;
                    self.last_event_time = instant;
                    self.last_event_type = Some(TraceEventType::Activation);

                    Some(Arrival::new(activation, final_cost, final_ss_time, final_ss_cnt))
                } else { // Account self-suspension
                    self.curr_ss_time += time_since_last_deactivation;
                    self.curr_ss_cnt += 1;
                    self.last_event_time = instant;
                    self.last_event_type = Some(TraceEventType::Activation);

                    None
                }
            },
            None => { 
                self.reset();
                self.activation = instant;
                self.last_event_time = instant;
                self.last_event_type = Some(TraceEventType::Activation);

                None
            },
            _ => { eprintln!("Invocation cycle warning: Last event type: {:#?} Pid {}", self.last_event_type.unwrap(), self.pid); self.reset(); None }
        }
    }

    fn resume(&mut self, instant:Time) {
        match self.last_event_type
        {
            Some (TraceEventType::Activation) => {
                assert!(instant >= self.last_event_time); // There can be an activation and resume at the same time
                self.last_event_type = Some(TraceEventType::Dispatch);
                self.last_event_time = instant;
            },
            Some (TraceEventType::Preemption) => {
                assert!(instant > self.last_event_time);
                self.last_event_type = Some(TraceEventType::Dispatch);
                self.last_event_time = instant;
            },
            None => self.reset(),
            _ => { eprintln!("Invocation cycle warning: Last event type: {:#?} Pid {}", self.last_event_type.unwrap(), self.pid); self.reset(); }
        }
    }

    fn preemption(&mut self, instant:Time) {
        if self.last_event_type.is_some() {
            assert!(instant > self.last_event_time);
        }

        match self.last_event_type
        {
            Some (TraceEventType::Dispatch) => {
                self.last_event_type = Some(TraceEventType::Preemption);
                self.curr_cost += instant - self.last_event_time;
                self.last_event_time = instant;
            }
            None => self.reset(),
            _ => { eprintln!("Invocation cycle warning: Last event type: {:#?} Pid {}", self.last_event_type.unwrap(), self.pid); self.reset(); }
        }
    }

    fn deactivation(&mut self, instant:Time) -> Option<Arrival> {
        if self.last_event_type.is_some() {
            assert!(instant > self.last_event_time);
        }

        match self.last_event_type
        {
            Some (TraceEventType::Dispatch) => {
                match self.heuristic {
                    // Every suspension ends a cycle
                    IcHeuristic::Suspension => { // End cycle
                        assert!(self.curr_ss_time == 0 && self.curr_ss_cnt == 0);
                        let activation = self.activation;
                        let final_cost = self.curr_cost + instant - self.last_event_time;
                        self.reset();

                        Some(Arrival::new(activation, final_cost, 0, 0))
                    },
                    // Must wait for next activation to decide if the cycle ended
                    IcHeuristic::SuspensionTimeout => {
                        self.last_event_type = Some(TraceEventType::Deactivation);
                        self.curr_cost += instant - self.last_event_time;
                        self.last_event_time = instant;

                        None
                    },
                }
            }
            None => { self.reset(); None },
            _ => { eprintln!("Invocation cycle warning: Last event type: {:#?} Pid {}", self.last_event_type.unwrap(), self.pid); self.reset(); None }
        }
    }

    fn exit(&mut self, instant:Time) -> Option<Arrival> {
        match self.last_event_type
        {
            Some(_) => { // End cycle
                assert!(instant > self.last_event_time);
                let activation = self.activation;
                let final_cost = if let Some (TraceEventType::Dispatch) = self.last_event_type 
                                    { self.curr_cost + instant - self.last_event_time }
                                else { self.curr_cost };
                let final_ss_time = self.curr_ss_time;
                let final_ss_cnt = self.curr_ss_cnt;
            
                self.reset();

                Some(Arrival::new(activation, final_cost, final_ss_time, final_ss_cnt))
            }
            None => { self.reset(); None }
        }
    }
}
