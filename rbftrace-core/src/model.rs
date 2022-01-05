use crate::sys_conf::SysConf;
use crate::time::{Pid, Time, ns_to_s};
use std::collections::{BTreeMap};

use serde::{Serialize, Deserialize};

use crate::rbf::RbfCurve;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
pub enum JobArrivalModel {
    Sporadic(Time),
    PeriodicJitter{period: Time, jitter: Time},
    PeriodicJitterOffset{period: Time, offset: Time, jitter: Time}
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq, Copy, Clone)]
pub struct ScalarTaskModel {
    // pub pid: Pid,
    pub execution_time: Time,
    pub arrival_model: JobArrivalModel
}

impl ScalarTaskModel {
    pub fn sporadic(execution_time: Time, mit: Time) -> ScalarTaskModel {
        ScalarTaskModel {
            arrival_model: JobArrivalModel::Sporadic(mit),
            execution_time: execution_time
        }
    }
    
    pub fn periodic_jitter(execution_time: Time, period: Time, jitter: Time) -> ScalarTaskModel {
        ScalarTaskModel {
            arrival_model: JobArrivalModel::PeriodicJitter{period, jitter},
            execution_time: execution_time
        }
    }
    
    pub fn periodic_jitter_offset(execution_time: Time, period: Time, jitter: Time, offset: Time) -> ScalarTaskModel {
        ScalarTaskModel {
            arrival_model: JobArrivalModel::PeriodicJitterOffset{period, offset, jitter},
            execution_time: execution_time
        }
    }

    pub fn pjo_to_pj(&self) -> Option<ScalarTaskModel> {
        match self.arrival_model {
            JobArrivalModel::PeriodicJitterOffset{period, jitter, ..} 
                => Some(ScalarTaskModel::periodic_jitter(self.execution_time, period, jitter)),
            JobArrivalModel::PeriodicJitter{period, jitter} 
                => Some(ScalarTaskModel::periodic_jitter(self.execution_time, period, jitter)),
            _ => None
        }
    }

    pub fn pretty_print(&self) {
        match self.arrival_model {
            JobArrivalModel::PeriodicJitterOffset{period, jitter, offset} => {
                println!("PJITTER-OFFSET");
                println!("    P = {}", ns_to_s(period));
                println!("    J = {}", ns_to_s(jitter));
                println!("    WCET = {}", ns_to_s(self.execution_time));
                println!("    OFFSET = {}", ns_to_s(offset));
            },
            JobArrivalModel::PeriodicJitter{period, jitter} => {
                println!("PJITTER");
                println!("    P = {}", ns_to_s(period));
                println!("    J = {}", ns_to_s(jitter));
                println!("    WCET = {}", ns_to_s(self.execution_time));
            },
            JobArrivalModel::Sporadic(mit) => {
                println!("SPORADIC");
                println!("    MIT = {}", ns_to_s(mit));
                println!("    WCET = {}", ns_to_s(self.execution_time));
            },
        }
    }


}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SystemModel {
    pub sys_conf: SysConf,
    pub scalar_models:  BTreeMap<Pid, ScalarTaskModel>,
    pub rbfs: BTreeMap<Pid, RbfCurve>
}

impl SystemModel {
    pub fn new(sys_conf: SysConf) -> SystemModel {
        SystemModel{
            sys_conf: sys_conf,
            scalar_models: BTreeMap::new(),
            rbfs: BTreeMap::new()
        }
    }

    pub fn get_sys_conf(&self) -> &SysConf{
        &self.sys_conf
    }

    pub fn get_scalar_models(&self, pid: Pid) -> Option<&ScalarTaskModel> {
        self.scalar_models.get(&pid)
    }

    pub fn set_scalar_model(&mut self, pid: Pid, model: ScalarTaskModel) {
        self.scalar_models.insert(pid, model);
    }

    pub fn get_rbf(&self, pid: Pid) -> Option<&RbfCurve>{
        self.rbfs.get(&pid)
    }
    
    pub fn set_rbf(&mut self, pid: Pid, rbf: RbfCurve) {
        self.rbfs.insert(pid, rbf);
    }

    pub fn pids(&self) -> impl Iterator<Item=&Pid> {
        self.scalar_models.keys()
    }
}