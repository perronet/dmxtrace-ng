use std::io::prelude::*;

use rbftrace_core::sys_conf::*;

use crate::system::*;
use crate::multiproc_type::*;
use rbftrace_core::util::*;

const FILTER_KTHREADS: bool = true;
const SMT_CHECK: bool = false;

pub fn detect_sys_conf() -> SysConf {
    if SMT_CHECK && !check_smt_disabled() && !check_smt_disabled_defacto() {
        eprint!("\nHyperthreading is enabled, please disable it.\
        \nRun 'echo off | sudo tee /sys/devices/system/cpu/smt/control' to disable manually.\
        \n\nWARNING: Doing this might leave some of the real-time processes with no affinity mask.\
        \nIn particular, those processes that had a mask containing only a logical processor\
        \nthat got disabled after running the above command.\
        \nThis is why it is recommended to disable hyperthreading through the BIOS\
        \nor with kernel parameter 'maxcpus=n', where n is the number of real cores.\n\n");
        write!(std::io::stderr(), "*** Do you want to disable hyperthreading and continue? (Press any key to continue) ***").unwrap();
        std::io::stderr().flush().unwrap();
        let _ = std::io::stdin().read(&mut [0u8]).unwrap();
        run_cmd("echo off | sudo tee /sys/devices/system/cpu/smt/control".to_string());
    }

    if FILTER_KTHREADS {
        eprintln!("Warning: kernel threads are being filtered.");
    }
    eprintln!();

    /*** Assume no hyperthreading from now on ***/

    let mut sys_conf = SysConf::default();
    
    sys_conf.multiproc = get_multiproc_type();
    sys_conf.n_cores = get_n_real_cores();
    sys_conf.rt_pids = get_pids_with_policy(vec!(SchedPolicy::FIFO, SchedPolicy::RR), false);
    // By default, analyze every real-time pid.
    sys_conf.target_pids = sys_conf.rt_pids.clone();
    sys_conf.fifo_pids = get_pids_with_policy(vec!(SchedPolicy::FIFO), false);
    sys_conf.rr_pids = get_pids_with_policy(vec!(SchedPolicy::RR), false);
    sys_conf.dl_pids = get_pids_with_policy(vec!(SchedPolicy::DEADLINE), false);
    sys_conf.kthread_pids = get_kthread_pids();

    set_rt_threads_info_and_clusters(&mut sys_conf, FILTER_KTHREADS);

    if !sys_conf.dl_pids.is_empty() {
        detect_dl_slack(&mut sys_conf);
    }

    sys_conf.rt_period = get_sched_rt_period_us();
    sys_conf.rt_runtime = get_sched_rt_runtime_us();

    if kernel_config_active("CONFIG_SCHED_DEBUG") {
        sys_conf.rt_runtime_is_global = sched_feat_active("RT_RUNTIME_SHARE");
        sys_conf.rt_runtime_is_greedy = sched_feat_active("RT_RUNTIME_GREED");
    }
    else {
        eprintln!("WARNING: CONFIG_SCHED_DEBUG is not set in the running kernel. There is no way to detect active scheduler features. These features will be detected as not active by default, but they might be active.");
    }

    detect_max_runtimes(&mut sys_conf);

    return sys_conf;
}
