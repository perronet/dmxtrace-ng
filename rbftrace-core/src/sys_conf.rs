use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::time::*;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SchedPolicy {
    CFS,
    FIFO,
    RR,
    BATCH,
    IDLE,
    DEADLINE,
    ERROR,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MultiprocType {
    PARTITIONED,
    GLOBAL,
    CLUSTERED,
    CLUSTEREDNF,
    APA,
    MIXED,
    ERROR,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SysConf { 
    pub multiproc : MultiprocType,
    pub n_cores: u32,

    /* Lists of pids */
    /// FIFO and RR threads
    pub rt_pids : Vec<Pid>,
    pub fifo_pids : Vec<Pid>,
    pub rr_pids : Vec<Pid>,
    pub dl_pids : Vec<Pid>,
    /// SCHED_DEADLINE threads with slack reclamation
    pub dl_slack_rec_pids : Vec<Pid>,
    /// Processes that must be analyzed, which is a subset of (rt_pids U dl_pids)
    pub target_pids : Vec<Pid>,                  
    pub kthread_pids : Vec<Pid>,                

    /* Lists of thread information */
    /// Thread-level attributes
    pub rt_threads_info: HashMap<Pid, ThreadInfo>,
    /// Thread-level attributes separated in clusters and ordered by decreasing priority
    /// When performing the analysis, it is more efficient to iterate on this vector
    pub rt_threads_info_clusters: Vec<Cluster>,

    /* Consecutive runtime limit */
    /// Processes with constrained runtime are present (RLIMIT_RTTIME)
    pub max_runtimes : bool,
    /// List of pids and their max consecutive runtime, if they have such limit
    pub procs_max_runtimes: Vec<RuntimeLimit>,

    /* Real-time throttling */
    /// RT period length
    pub rt_period : i32,
    /// RT max runtime in period before throttling
    pub rt_runtime : i32,
    /// The rt_runtime limit can be per-core or global (RT_RUNTIME_SHARE)
    pub rt_runtime_is_global : bool,
    /// The rt_runtime limit can be avoided if there are no threads to starve (RT_RUNTIME_GREED)
    pub rt_runtime_is_greedy : bool,
}

impl Default for SysConf {
    fn default() -> Self {
        SysConf {
            multiproc: MultiprocType::ERROR,
            n_cores: 0,
            rt_pids : Vec::default(),
            fifo_pids : Vec::default(),
            rr_pids : Vec::default(),
            dl_pids : Vec::default(),
            dl_slack_rec_pids : Vec::default(),
            target_pids : Vec::default(),
            kthread_pids : Vec::default(),
            rt_threads_info : HashMap::default(),
            rt_threads_info_clusters : Vec::default(),
            max_runtimes : false,
            rt_period : 0,
            rt_runtime : 0,
            rt_runtime_is_global : false,
            rt_runtime_is_greedy : false,
            procs_max_runtimes: Vec::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadInfo {
    pub pid: Pid,
    pub prio: Priority,
    pub policy: SchedPolicy,
    pub affinity: Vec<Cpu>, 
    pub is_target: bool,
    pub is_kthread: bool
}

impl Default for ThreadInfo {
    fn default() -> Self { 
        ThreadInfo {
            pid: 0,
            prio: 0,
            policy: SchedPolicy::ERROR,
            affinity: Vec::new(), 
            is_target: false,
            is_kthread: false    
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Cluster {
    pub id: u32,
    pub cpus: Vec<Cpu>,
    /// Must be ordered by decreasing priority for efficiency in the analysis
    pub threads: Vec<ThreadInfo>,
}

impl Cluster {
    pub fn new(id: u32, cpus: Vec<Cpu>, threads: Vec<ThreadInfo>) -> Self {
        Cluster {
            id,
            cpus,
            threads,
        }
    }
}

/* Hard consecutive runtime limit imposed on a process. */
/* If the process runs without self-suspending for this time, it will be killed */
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeLimit {
    pub pid: Pid,
    pub max_runtime: u64,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct Core {
    pub id: Cpu,
    pub logical_cpu_ids: Vec<Cpu>,
}
