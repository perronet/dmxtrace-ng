use std::fs;
use sysinfo::{System, SystemExt};
use std::convert::TryInto;

use rbftrace_core::trace::*;
use rbftrace_core::time::*;

/* C wrappers */
use crate::ffi::trace_cmd;

pub struct FTraceEVG {
    /// The set of traced pids. Typically every real-time thread in the system
    rt_pids: Vec<Pid>,
    /// A subset of rt_pids that represents the pids under analysis
    target_pids: Vec<Pid>,
    /// Number of target pids that exited
    exit_cnt: u64,
    /// Number of processes events where a target pid was involved
    /// Events that do not involve a target pid are discarded
    /// For example, two non-target pids preempting each other
    processed_events: u64,
    /// Total number of processed events
    processed_events_all: u64,
    /// Time at which the first event was consumed by using next_event()
    start_time: std::time::Instant,
    /// If a raw event is a context switch, two events are produced:
    /// A Preemption for the first pid, and a Dispatch for the second.
    /// This means that a single read from the pipe can produce two events,
    /// so before doing another read, we check if there was an extra event
    /// produced in the previous read
    extra_event: Option<TraceEvent>,

    recorders_stopped: bool,
    
    /*** User-defined parameters ***/

    /// If > 0 tracing is performed only for "duration" seconds
    /// By default, tracing is done until all target pids are dead (duration = 0)
    duration: u64, // In seconds
    /// Size of the ftrace ring buffer in kb
    ftrace_bufsize: u32,

    /* Needed to parse the events */
    ids: EventsId,

    /* Needed by the C functions to read the stream. There is no reason to ever touch these. */
    tracefs: *mut trace_cmd::tracefs_instance,
    recorders: *mut trace_cmd::recorder_data,
    cpu_cnt: i32,
}

impl FTraceEVG {

    /* When this function returns None, tracing is stopped */
    pub fn next_event(&mut self) -> Option<TraceEvent> {
        let mut result: Option<TraceEvent>;

        if self.processed_events == 0 {
            self.start_time = std::time::Instant::now();
        }

        loop {
            /* Continue to flush the remaining contents of the pipes.
               After this, read_stream_parse() returns Some until the pipes are empty */
            if !self.recorders_stopped && 
                (self.targets_are_dead() || 
                self.duration > 0 && self.duration_reached()) {

                self.disable_tracing_stop_recorders();
            }

            /*** Get single event ***/
            if self.extra_event.is_some() {
                /* If a raw event is a context switch, two events are produced:
                   A Preemption for the first pid, and a Dispatch for the second.
                   This means that a single read from the pipe can produce two events,
                   so before doing another read, we check if there was an extra event
                   produced in the previous read. */
                result = self.extra_event;
                self.extra_event = None;
            } else {
                /* The pipe is non-blocking. If None is returned we must try again, 
                   unless the recorders are stopped.
                   In that case, we know that nothing will ever be written to the pipe. */
                result = self.read_stream_parse();
            }

            /*** Check if event is relevant. If not, read another. ***/
            if result.is_some() {
                self.processed_events_all += 1;
                if !self.target_pids.contains(&result.unwrap().pid) {
                    continue;
                }
                self.processed_events += 1;

                if result.unwrap().etype == TraceEventType::Exit {
                    self.exit_cnt += 1; // TODO might be useless, the idea is to avoid parsing after the exit events have been observed, and just flush the pipes
                }

                return result;
            } else if self.recorders_stopped {
                break;
            }
        }

        assert!(self.recorders_stopped);
        // TODO proper ctrl+c handler for shutdown
        trace_cmd::wait_recorder_threads(self.recorders, self.cpu_cnt);

        None
    }
    
    pub fn setup(&mut self) {
        /*** Clean ***/
        trace_cmd::stop_tracing(self.tracefs);
        trace_cmd::set_monotonic_clock(self.tracefs); // This also clears the trace
        trace_cmd::clear_pids(self.tracefs);

        /*** Setup ***/
        /* Child processes are traced too + on every pid tracing stops if the process exits */
        trace_cmd::set_event_fork(self.tracefs);
        trace_cmd::set_pids(self.tracefs, &self.rt_pids);
        trace_cmd::set_events(self.tracefs);
        trace_cmd::set_buffer_size(self.tracefs, self.ftrace_bufsize);

        /*** Activate tracing ***/
        trace_cmd::start_tracing(self.tracefs);

        /*** Cleanup on ctrl+C ***/
        ctrlc::set_handler(||{sigint_handle();}).expect("");
    }

    pub fn shutdown(&mut self) {
        if !self.recorders_stopped {
            self.disable_tracing_stop_recorders();
            trace_cmd::wait_recorder_threads(self.recorders, self.cpu_cnt);
        }

        trace_cmd::stop_tracing(self.tracefs);
        trace_cmd::clear_trace(self.tracefs);
        trace_cmd::clear_pids(self.tracefs);
        trace_cmd::clear_events(self.tracefs);
        trace_cmd::clear_event_fork(self.tracefs);
        trace_cmd::destroy_tracefs(self.tracefs);

        println!("TRACING: Done! Processed events: {} Total events: {}", self.processed_events, self.processed_events_all);
    }
}

impl FTraceEVG {
    pub fn new(target_pids: &Vec<Pid>, rt_pids: &Vec<Pid>, duration: u64, bufsize: u32) -> Self {
        let s = System::new();
        let cpu_cnt: i32 = s.processors().len().try_into().unwrap();
        let tracefs = trace_cmd::create_tracefs();
        let recorders = trace_cmd::init_recorders(tracefs, cpu_cnt);
        
        FTraceEVG {
            target_pids: target_pids.clone(),
            rt_pids: rt_pids.clone(),
            exit_cnt: 0,
            processed_events: 0,
            processed_events_all: 0,
            start_time: std::time::Instant::now(), // Will be set when reading the first event
            recorders_stopped: false,
            extra_event: None,
            
            duration: duration,
            ftrace_bufsize: bufsize,

            ids: EventsId::from_tracefs(tracefs),

            tracefs: tracefs,
            recorders: recorders,
            cpu_cnt: cpu_cnt,
        }
    }

    fn read_stream_parse(&mut self) -> Option<TraceEvent> {
        if let Some(raw_event) = trace_cmd::read_stream_raw(self.recorders, self.cpu_cnt) {
            let trace_event = Some(self.event_from_raw(&raw_event));

            return trace_event;
        }

        None
    }

    fn disable_tracing_stop_recorders(&mut self) {
        trace_cmd::stop_tracing(self.tracefs);
        trace_cmd::stop_recorder_threads(self.recorders, self.cpu_cnt);
        self.recorders_stopped = true;
    }

    fn duration_reached(&mut self) -> bool {
        self.start_time.elapsed().as_secs() >= self.duration
    }

    fn targets_are_dead(&mut self) -> bool {
        let s = System::new_all();
        for pid in &self.target_pids {
            let pid_int = (*pid).try_into().unwrap();
            if s.process(pid_int).is_some() {
                return false;
            }
        }

        true
    }

    /* Parse raw events */
    // Not that we are discarding most of the fields
    fn event_from_raw(&mut self, raw_event: &trace_cmd::rbftrace_event_raw) -> TraceEvent {
        let raw_type = match raw_event.id {
            id if id == self.ids.sched_switch_id => TraceEventTypeRaw::Switch,
            id if id == self.ids.sched_wakeup_id => TraceEventTypeRaw::Wakeup,
            id if id == self.ids.sched_wakeup_new_id => TraceEventTypeRaw::Wakeup,
            id if id == self.ids.sched_process_exit_id => TraceEventTypeRaw::Exit,
            _ => { panic!("Bad event id.") }
        };

        /* https://elixir.bootlin.com/linux/v5.6/source/include/trace/events/sched.h#L167 */
        match raw_type {
            TraceEventTypeRaw::Switch => {
                let prev_pid = raw_event.pid as u32;
                let next_pid = raw_event.next_pid as u32;
                let event_type: TraceEventType;

                // Either Preemption or Deactivation
                if is_preemption(raw_event) {
                    event_type = TraceEventType::Preemption;
                } else {
                    event_type = TraceEventType::Deactivation;
                }
                let event_1 = TraceEvent::new(event_type, prev_pid, raw_event.ts);

                // Dispatch
                let event_2 = TraceEvent::new(TraceEventType::Dispatch, next_pid, raw_event.ts);

                self.extra_event = Some(event_2);
                return event_1;
            },
            TraceEventTypeRaw::Wakeup => {
                return TraceEvent::new(TraceEventType::Activation, raw_event.pid as u32, raw_event.ts);
            },
            TraceEventTypeRaw::Exit => {
                return TraceEvent::new(TraceEventType::Exit, raw_event.pid as u32, raw_event.ts);
            },
        };
    }
}

/* SUPPORT */

#[derive(PartialEq, Debug, Copy, Clone)]
pub enum TraceEventTypeRaw {
    Switch,
    Wakeup,
    Exit,
}

/* These ids are machine-dependent, so we read them from tracefs. */
#[derive(Debug, Copy, Clone)]
pub struct EventsId {
    sched_switch_id: u16,
    sched_wakeup_id: u16,
    sched_wakeup_new_id: u16,
    sched_process_exit_id: u16,
}

impl EventsId {
    pub fn from_tracefs(tracefs: *mut trace_cmd::tracefs_instance) -> Self {
        EventsId {
            sched_switch_id: trace_cmd::get_event_id(tracefs, "sched_switch"),
            sched_wakeup_id: trace_cmd::get_event_id(tracefs, "sched_wakeup"),
            sched_wakeup_new_id: trace_cmd::get_event_id(tracefs, "sched_wakeup_new"),
            sched_process_exit_id: trace_cmd::get_event_id(tracefs, "sched_process_exit"),
        }
    }
}

// If the bitmask for process states is changed, this will break
/* https://elixir.bootlin.com/linux/v5.6/source/include/linux/sched.h#L76 */
fn is_preemption(raw_event: &trace_cmd::rbftrace_event_raw) -> bool {
    // The *current* state of the previous process is "Runnable"
    return raw_event.prev_state == 0 || raw_event.prev_state == 256;
}

/* Cleanup on ctrl+C */
// TODO this is very ugly, but rust won't let us use the EVG instance because of the presence of raw pointers
fn sigint_handle() {
    // Stop tracing
    fs::write("/sys/kernel/debug/tracing/tracing_on", "0").expect("Can't write to file 'tracing_on'");
    // Clear trace
    fs::write("/sys/kernel/debug/tracing/trace", " ").expect("Can't write to file 'trace'");
    std::process::exit(0);
}

/* Cleanup on panic */
impl Drop for FTraceEVG {
    fn drop(&mut self) {
        self.shutdown();
    }
}
