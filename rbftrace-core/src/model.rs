use crate::sys_conf::SysConf;
use crate::time::{Time};
use std::collections::{BTreeMap};
use crate::sys_conf::{Pid};

use serde::{Serialize, Deserialize};

#[derive(Clone, Copy)]
pub struct Job {
    pub execution_time: Time,
    pub arrived_at: Time,
    pub completed_at: Time,
    pub preemption_time: Time,
}

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Default, Clone, Copy)]
pub struct PeriodicTask {
    pub period: Time,
    pub offset: Time,
    pub jitter: Time,
    pub wcet: Time
}

impl PeriodicTask {
    pub fn new(period: Time, jitter: Time, offset: Time, wcet:Time) -> Self {
        Self {
            period, 
            jitter, 
            offset,
            wcet
        }
    }

    pub fn pretty_print(&self) {
        if self.jitter.is_zero() {
            println!("PJITTER");
            println!("    P = {}", (self.period.to_s()));
            println!("    J = {}", (self.jitter.to_s()));
            println!("    WCET = {}", (self.wcet.to_s()));
        }
        else {
            println!("PJITTER-OFFSET");
            println!("    P = {}", (self.period.to_s()));
            println!("    J = {}", (self.jitter.to_s()));
            println!("    WCET = {}", (self.wcet.to_s()));
            println!("    OFFSET = {}", (self.offset.to_s()));
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SystemModel<T> {
    sys_conf: SysConf,
    models: BTreeMap<Pid, T>
}

impl<T> SystemModel<T> {
    pub fn new(sys_conf: SysConf) -> Self {
        Self {
            sys_conf,
            models : BTreeMap::new(),
        }
    }

    pub fn get_sys_conf(&self) -> &SysConf{
        &self.sys_conf
    }

    pub fn get_model(&self, pid: Pid) -> Option<&T> {
        self.models.get(&pid)
    }

    pub fn set_task_model(&mut self, pid: Pid, model: T) {
        self.models.insert(pid, model);
    }

    pub fn pids(&self) -> impl Iterator<Item=&Pid> {
        self.models.keys()
    }
}