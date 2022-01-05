use std::path::Path;

use serde::{Serialize, Deserialize};
use serde_yaml;

use crate::time::*;

#[derive(PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub enum TraceEventType {
    Activation,       // Enter runqueue
    Deactivation,     // Exit runqueue
    Preemption,       // Context switched out
    Dispatch,           // Context switched in
    Exit,             // Exited
}

impl TraceEventType {
    pub fn short_name(&self) -> char {
        match self {
            TraceEventType::Activation => 'A',
            TraceEventType::Deactivation => 'D',
            TraceEventType::Preemption => 'P',
            TraceEventType::Dispatch => 'R',
            TraceEventType::Exit => 'E',
        }
    }
}

#[derive(PartialEq, Eq, Debug, Copy, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
    pub etype : TraceEventType,
    pub pid : Pid,
    pub instant : Time,
}

impl TraceEvent {
    pub fn new(etype : TraceEventType, pid : Pid, instant : Time) -> Self {
        TraceEvent { etype, pid, instant }
    }
    pub fn activation(pid: Pid, instant : Time) -> Self {
        TraceEvent::new(TraceEventType::Activation, pid, instant)
    }
    
    pub fn deactivation(pid: Pid, instant : Time) -> Self {
        TraceEvent::new(TraceEventType::Deactivation, pid, instant)
    }
    
    pub fn dispatch(pid: Pid, instant : Time) -> Self {
        TraceEvent::new(TraceEventType::Dispatch, pid, instant)
    }
    
    pub fn preemption(pid: Pid, instant : Time) -> Self {
        TraceEvent::new(TraceEventType::Preemption, pid, instant)
    }
    
    pub fn exit(pid: Pid, instant : Time) -> Self {
        TraceEvent::new(TraceEventType::Exit, pid, instant)
    }
}

#[derive(PartialEq, Eq, Debug)]
pub struct Trace {
    events: Vec<TraceEvent>
}

#[derive(Debug)]
pub enum TraceError {
    Monotonocity{pos: usize, prev: TraceEvent, event: TraceEvent},
    IO(std::io::Error),
    YAMLParsing(serde_yaml::Error)
}

impl Trace {
    pub fn new() -> Trace {
        Trace{
            events: vec![]
        }
    }

    pub fn events(&self) -> impl Iterator<Item=&TraceEvent> {
        self.events.iter()
    }

    // trace specific logic for instance

    pub fn push(&mut self, e: TraceEvent) -> Result<(), TraceError> {
        if let Some(prev) = self.events().last() {
            // Activation and Dispatch can have the same timestamp
            // This happens when the thread gets immediately scheduled
            if prev.instant > e.instant {
                let pos = self.events.len();
                return Err(TraceError::Monotonocity{pos: pos, prev: *prev, event: e})
            }
        }

        self.events.push(e);

        Ok(())  
    }

    pub fn from_yaml_file<P: AsRef<Path>>(path: P) -> Result<Trace, TraceError> {
        let mut ret = Trace::new();

        let f = std::fs::read_to_string(path);

        match f {
            Err(e) => return Err(TraceError::IO(e)),
            Ok(s) => {
                match serde_yaml::from_str::<Vec<TraceEvent>>(s.as_str()) {
                    Err(e) => return Err(TraceError::YAMLParsing(e)),
                    Ok(v) => {
                        for event in v {
                            ret.push(event)?;
                        }
                    }
                }
            }
        }

        return Ok(ret)
    }
}

impl<T> From<T> for Trace 
where T: AsRef<[TraceEvent]>
{
    fn from(events: T) -> Self { 
        Trace {
            events: Vec::from(events.as_ref())
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::{TraceEvent, TraceEventType, Trace, TraceError};

    #[test]
    pub fn test_from() {
        let trace = Trace::from([
            TraceEvent::new(TraceEventType::Activation, 0, 1),
            TraceEvent::new(TraceEventType::Activation, 0, 4),
            TraceEvent::new(TraceEventType::Activation, 0, 7),
            TraceEvent::new(TraceEventType::Activation, 0, 9)
        ]);

        let mut events = trace.events();
        
        assert_eq!(events.next(), Some(&TraceEvent::new(TraceEventType::Activation, 0, 1)));
        assert_eq!(events.next(), Some(&TraceEvent::new(TraceEventType::Activation, 0, 4)));
        assert_eq!(events.next(), Some(&TraceEvent::new(TraceEventType::Activation, 0, 7)));
        assert_eq!(events.next(), Some(&TraceEvent::new(TraceEventType::Activation, 0, 9)));
        assert_eq!(events.next(), None);
    }

    #[test]
    pub fn test_push() -> Result<(), TraceError> {
        let mut trace = Trace::new();
 
        trace.push(TraceEvent::new(TraceEventType::Activation, 0, 1))?;
        trace.push(TraceEvent::new(TraceEventType::Activation, 0, 2))?;
        trace.push(TraceEvent::new(TraceEventType::Activation, 0, 3))?;

        assert!(trace.push(TraceEvent::new(TraceEventType::Activation, 0, 1)).is_err());
        
        Ok(())
    }

    #[test]
    pub fn test_eq() {
        let t1 = Trace::from([
            TraceEvent::new(TraceEventType::Activation, 0, 1),
            TraceEvent::new(TraceEventType::Activation, 0, 4),
            TraceEvent::new(TraceEventType::Activation, 0, 7),
            TraceEvent::new(TraceEventType::Activation, 0, 9)
        ]);

        let t2 = Trace::from([
            TraceEvent::new(TraceEventType::Activation, 0, 1),
            TraceEvent::new(TraceEventType::Activation, 0, 4),
            TraceEvent::new(TraceEventType::Activation, 0, 7),
            TraceEvent::new(TraceEventType::Activation, 0, 9)
        ]);

        let t3 = Trace::from([
            TraceEvent::new(TraceEventType::Activation, 0, 1),
            TraceEvent::new(TraceEventType::Activation, 0, 7),
            TraceEvent::new(TraceEventType::Activation, 0, 9)
        ]);
        
        let t4 = Trace::from([
            TraceEvent::new(TraceEventType::Activation, 0, 7),
            TraceEvent::new(TraceEventType::Activation, 0, 1),
            TraceEvent::new(TraceEventType::Activation, 0, 9),
            TraceEvent::new(TraceEventType::Activation, 0, 4),
        ]);

        assert_eq!(t1, t1);
        assert_eq!(t1, t2);
        assert_ne!(t1, t3);
        assert_ne!(t1, t4);
        assert_ne!(t3, t4);
    }

    #[test]
    pub fn test_yaml_parsing() -> Result<(), TraceError> {
        let p = "test_input/trace_1.txt";

        let t = Trace::from_yaml_file(p)?;
        
        let expected = Trace::from([
            TraceEvent::new(TraceEventType::Activation, 1, 1),
            TraceEvent::new(TraceEventType::Dispatch, 1, 2),
            TraceEvent::new(TraceEventType::Preemption, 1, 3),
            TraceEvent::new(TraceEventType::Deactivation, 1, 4),
            TraceEvent::new(TraceEventType::Exit, 1, 5)
        ]);

        assert_eq!(t, expected);

        Ok(())
    }
}