use std::mem::size_of;
use nc;
use std::fs::*;

use rbftrace_core::sys_conf::*;
use rbftrace_core::time::*;
use rbftrace_core::util::*;

pub fn get_cpu_topology() -> Vec<Core> {
    let to_parse = run_cmd("lscpu -p=core,cpu | sed '1,4d' | sort".to_string());
    let mut r: Vec<Core> = Vec::new();
    let mut core_id: Cpu = 0;
    let mut logical_id: Cpu = 0;
    let n_cores = run_cmd("lscpu -p=core | sed '1,4d' | sort | uniq".to_string()).lines().count();
    for i in 0..n_cores {
        let mut core = Core::default();
        core.id = i as Cpu;
        r.push(core);
    }

    for line in to_parse.lines() {
        for (i, s) in line.split(",").enumerate() {
            if i == 0 {
                core_id = s.trim().parse::<Cpu>().unwrap();
            }
            else {
                logical_id = s.trim().parse::<Cpu>().unwrap();
            }
        }
        r[core_id as usize].logical_cpu_ids.push(logical_id);
    }

    return r;
}

/* Number of installed cores, this includes offline cores and logical cores */
pub fn get_nproc() -> u32 {
    return run_cmd("nproc --all".to_string()).trim().parse::<u32>().unwrap();
}

/* Number of real cores */
pub fn get_n_real_cores() -> u32 {
    return get_cpu_topology().len() as u32;
}

pub fn get_rt_pids() -> Vec<Pid> {
    return get_pids_with_policy(vec!(SchedPolicy::FIFO, SchedPolicy::RR), false);
}

pub fn get_pids_with_policy(policies: Vec<SchedPolicy>, print: bool) -> Vec<Pid> {
    let mut ret_pids = Vec::new();
    let all_pids = run_cmd("ps -A -L -o lwp=".to_string()); // Includes threads (-L and lwp)

    if print {
        println!("Processes with policies {:?}:", policies);
    }
    for line in all_pids.lines() {
        let pid = line.trim().parse::<Pid>().unwrap();
        let policy = get_policy(pid);

        if policies.contains(&policy) {
            if print {
                println!("{} - {} - {:?} - prio {} - affinity {:?}", pid, run_cmd(format!("ps -p {} -o comm=", pid)).trim(), policy, get_priority(pid), get_affinity(&pid).unwrap());
            }
            ret_pids.push(pid);
        }
    }
    return ret_pids;
}

pub fn set_rt_threads_info_and_clusters(mut sys_conf: &mut SysConf, filter_kthreads: bool) {
    if sys_conf.multiproc != MultiprocType::APA && sys_conf.multiproc != MultiprocType::ERROR {
        set_clusters(&mut sys_conf);
        assert!(!sys_conf.rt_threads_info_clusters.is_empty());
    }

    for pid in &sys_conf.rt_pids {
        let thread_info = get_thread_info(*pid, &sys_conf);

        if filter_kthreads && thread_info.is_kthread {
            continue;
        }

        // Sort into the correct cluster
        for cluster in sys_conf.rt_threads_info_clusters.iter_mut() {
            for cpu in &thread_info.affinity {
                if cluster.cpus.contains(&cpu) { 
                    cluster.threads.push(thread_info.clone());
                    break;
                }
            }
        }

        sys_conf.rt_threads_info.entry(*pid).or_insert(thread_info);
    }

    // Sort threads by priority, from high to low
    for cluster in sys_conf.rt_threads_info_clusters.iter_mut() {
        cluster.threads.sort_by(|a, b| b.prio.cmp(&a.prio));
    }
}

pub fn get_thread_info(pid: Pid, sys_conf: &SysConf) -> ThreadInfo {
    let mut info = ThreadInfo::default();
    let affinity = get_affinity(&pid);

    info.pid = pid;
    info.prio = get_priority(pid);
    info.policy = get_policy(pid);
    info.affinity = match affinity {
        Some(a) => a,
        None => Vec::new(),
    };
    info.is_target = sys_conf.target_pids.contains(&pid);
    info.is_kthread = sys_conf.kthread_pids.contains(&pid);

    return info;
}

pub fn set_clusters(sys_conf: &mut SysConf) {
    match sys_conf.multiproc {
        MultiprocType::GLOBAL => {
            let mut all_cpus = Vec::new();
            for i in 0..sys_conf.n_cores {
                all_cpus.push(i);
            }
            sys_conf.rt_threads_info_clusters.push(Cluster::new(0, all_cpus, Vec::new()));
        },
        MultiprocType::PARTITIONED => {
            for i in 0..sys_conf.n_cores {
                let mut one_cpu = Vec::new();
                one_cpu.push(i);
                sys_conf.rt_threads_info_clusters.push(Cluster::new(i, one_cpu, Vec::new()));
            }
        },
        MultiprocType::CLUSTERED => {
            sys_conf.rt_threads_info_clusters = crate::multiproc_type::check_clustered(&sys_conf.rt_pids, true).unwrap();
        },
        MultiprocType::CLUSTEREDNF => {
            sys_conf.rt_threads_info_clusters = crate::multiproc_type::check_clustered(&sys_conf.rt_pids, false).unwrap();
        },
        _ => { /* No clusters */ }
    }
}

pub fn get_priority(pid: Pid) -> Priority {
    let mut attrbuf = default_attr_t();
    match nc::sched_getattr(pid as i32, &mut attrbuf, size_of::<nc::sched_attr_t>() as u32, 0x0) {
        Err(e) => {
            eprintln!("{} failed to get priority, errno: {}", pid, e);
        },
        Ok(_) => {},
    };
    return attrbuf.sched_priority;
}

pub fn get_policy(pid: Pid) -> SchedPolicy {
    let mut attrbuf = default_attr_t();
    match nc::sched_getattr(pid as i32, &mut attrbuf, size_of::<nc::sched_attr_t>() as u32, 0x0) {
        Err(e) => {
            eprintln!("{} failed to get policy, errno: {}", pid, e);
        },
        Ok(_) => {},
    };
    let policy = match attrbuf.sched_policy {
        0 => SchedPolicy::CFS,
        1 => SchedPolicy::FIFO,
        2 => SchedPolicy::RR,
        3 => SchedPolicy::BATCH,
        5 => SchedPolicy::IDLE,
        6 => SchedPolicy::DEADLINE,
        _ => SchedPolicy::ERROR,
    };

    return policy;
}

pub fn get_kthread_pids() -> Vec<Pid> {
    let mut ret = Vec::new();
    /* Kernel threads are all children of pid 2 (kthreadd) */
    let kthreads_str = run_cmd("ps --ppid 2 -p 2 -o pid | tail -n +2".to_string());

    for line in kthreads_str.lines() {
        let pid = line.trim().parse::<Pid>().unwrap();
        ret.push(pid);
    }

    return ret;
}

/* After disabling SMT, kthreads that were bound to logical processors will 
   have an empty affinity mask and will stop running. Despite this, they will
   still be listed in the pids: it is necessary to ignore them, as they are
   not even running and they "appear" to be bound to disabled cores. */
pub fn filter_ht_pinned_kthreads(pids: &Vec<Pid>) -> Vec<Pid> {
    let mut r: Vec<Pid> = pids.clone();
    let kthreads = get_kthread_pids();
    
    /* Consider only threads in the input list */
    for pid in kthreads {
        if pids.contains(&pid) && get_affinity(&pid).unwrap().is_empty() {
            r.remove(r.iter().position(|x| *x == pid).unwrap());
        }
    }
    return r;
}

/* Remove every kthread pinned to a core that's unmovable */
pub fn filter_unmovable_pinned_kthreads(pids: &Vec<Pid>) -> Vec<Pid> {
    let mut r: Vec<Pid> = pids.clone();
    let kthreads = get_kthread_pids();

    /* Consider only threads in the input list */
    for pid in kthreads {
        if pids.contains(&pid) && is_unmovable_pinned(pid) {
            r.remove(r.iter().position(|x| *x == pid).unwrap());
        }
    }
    return r;
}

pub fn filter_non_rt_tasks(pids: &Vec<Pid>, rt_pids: &Vec<Pid>) -> Vec<Pid> {
    let mut r: Vec<Pid> = Vec::new();
    for pid in pids {
        if rt_pids.contains(&pid) {
            r.push(*pid);
        }
    }
    return r;
}

pub fn is_unmovable_pinned(pid: Pid) -> bool {
    let affinity = get_affinity(&pid).unwrap();
    if affinity.len() != 1 {
        return false;
    }
    /* Try to move and see what happens, works even if it's the same cpu */
    let r = match set_affinity(&pid, affinity) {
        Err(_) => true,
        Ok(_) => false,
    };
    return r;
}

/* Directly check on/off switch */
pub fn check_smt_disabled() -> bool {
    let smt_status = read_to_string("/sys/devices/system/cpu/smt/control");
    match smt_status {
        Ok(s) => {
            if s == "off" || s == "forceoff" || s == "notsupported" {
                return true;
            }
            else {
                return false;
            }
        },
        Err(_) => return false,
    }
}

/* SMT might be turned off another way: e.g. by manually turning off cpus or through BIOS. */
/* It might even be the case that the system does not support SMT */
pub fn check_smt_disabled_defacto() -> bool {
    for core in get_cpu_topology() {
        if core.logical_cpu_ids.len() != 1 {
            return false;
        }
    }
    return true;
}

pub fn check_throttling_disabled() -> bool {
    return get_sched_rt_runtime_us() == -1;
}

pub fn get_affinity(pid: &Pid) -> Option<Vec<Cpu>> {
    let mask: i64 = 0;
    let mask_ptr: *const i64 = &mask;
    let r = match nc::sched_getaffinity(*pid as i32, size_of::<usize>() as u32, mask_ptr as usize) {
        Err(_) => None,
        Ok(_) => Some(mask_to_cpu_vec(mask)),
    };
    return r;
}

pub fn set_affinity(pid: &Pid, mask_vec: Vec<Cpu>) -> Result<(), i32> {
    let mut mask: usize = cpu_vec_to_mask(mask_vec) as usize;
    return nc::sched_setaffinity(*pid as i32, size_of::<usize>() as u32, &mut mask);
}

pub fn mask_to_cpu_vec(mask: i64) -> Vec<Cpu> {
    let mut r: Vec<Cpu> = Vec::new();
    let n_cores = get_nproc();
    for i in 0..n_cores {
        if (mask & 2i64.pow(i)) == 2i64.pow(i) {
            r.push(i);
        }
    }
    return r;
}

pub fn cpu_vec_to_mask(mask_vec: Vec<Cpu>) -> i64 {
    let mut r: i64 = 0;
    let n_cores = get_nproc();
    for i in 0..n_cores {
        if mask_vec.contains(&i) {
            r += 2i64.pow(i)
        }
    }
    return r;
}

pub fn all_cpu_mask_vec() -> Vec<Cpu> {
    let mut r: Vec<Cpu> = Vec::new();
    let topology = get_cpu_topology();
    for core in topology {
        r.push(core.id);
    }
    return r;
}

pub fn get_sched_rt_period_us() -> i32 {
    return read_to_string("/proc/sys/kernel/sched_rt_period_us").unwrap().trim().parse::<i32>().unwrap();
}

pub fn get_sched_rt_runtime_us() -> i32 {
    return read_to_string("/proc/sys/kernel/sched_rt_runtime_us").unwrap().trim().parse::<i32>().unwrap();
}

pub fn detect_dl_slack(sys_conf: &mut SysConf) {
    let mut attrbuf = default_attr_t();
    for pid in &sys_conf.dl_pids {
        match nc::sched_getattr(*pid as nc::pid_t, &mut attrbuf, size_of::<nc::sched_attr_t>() as u32, 0x0) {
            Err(e) => {
                eprintln!("[DL] {} failed to get scheduling attributes, errno: {}", pid, e);
            },
            Ok(_) => {},
        };
        if (attrbuf.sched_flags & nc::SCHED_FLAG_RECLAIM as u64) == nc::SCHED_FLAG_RECLAIM  as u64 {
            sys_conf.dl_slack_rec_pids.push(*pid);
        }
    }
}

pub fn detect_max_runtimes(sys_conf: &mut SysConf) {
    for pid in &sys_conf.rt_pids {
        let limit = get_max_consecutive_runtime(*pid);
        if limit < nc::RLIM_INFINITY as u64 {
            let run_limit = RuntimeLimit {
                pid : *pid,
                max_runtime : limit,
            };
            sys_conf.procs_max_runtimes.push(run_limit);
            sys_conf.max_runtimes = true;
        }
    }
}

pub fn get_max_consecutive_runtime(pid: Pid) -> u64 {
    let mut old_limit = nc::rlimit64_t::default();
    match nc::prlimit64(pid as nc::pid_t, nc::RLIMIT_RTTIME, None, Some(&mut old_limit)) {
        Ok(_) => {},
        Err(e) => { println!("prlimit: ERRNO {}", e) },
    }
    return old_limit.rlim_max;
}

/* Check for scheduling features elixir.bootlin.com/linux/latest/source/kernel/sched/features.h */
/* Need CONFIG_SCHED_DEBUG active for the "sched_features" file to exist! */
pub fn sched_feat_active(feat: &str) -> bool {
    if kernel_config_active("CONFIG_SCHED_DEBUG") {
        let sched_features = read_to_string("/sys/kernel/debug/sched_features").unwrap();
        for f in sched_features.split_whitespace() {
            if f == feat { return true; }
        }
    }
    else {
        eprintln!("WARNING: CONFIG_SCHED_DEBUG is not set in the running kernel.");
    }
    return false;
}

/* There are 3 ways to fetch the kernel configuration, depending on the system: try all 3 */
pub fn kernel_config_active(opt: &str) -> bool {
    let mut config = String::new();
    let config_try_1 = run_cmd_opt("cat /boot/config-$(uname -r)".to_string());
    let config_try_2 = run_cmd_opt("cat /boot/config".to_string());
    if config_try_1 == None && config_try_2 == None {
        run_cmd("modprobe configs".to_string());
        let config_try_3 = run_cmd_opt("zcat /proc/config.gz".to_string());
        if config_try_3 != None { 
            config = config_try_3.unwrap();
        }
    } 
    else {
        config = if config_try_1 == None { config_try_2.unwrap() } else { config_try_1.unwrap() };
    }
    if !config.is_empty() {
        for line in config.lines() {
            if line.trim() == format!("{}=y", opt) || line.trim() == format!("{}=m", opt) {
                return true;
            }
        }
    }
    return false;
}

pub fn default_attr_t() -> nc::sched_attr_t {
    nc::sched_attr_t {
        size: 0,
        sched_policy: 0,
        sched_flags: 0,
        sched_nice: 0,
        sched_priority: 0,
        sched_runtime: 0,
        sched_deadline: 0,
        sched_period: 0,
        sched_util_min: 0,
        sched_util_max: 0,
    }
}
