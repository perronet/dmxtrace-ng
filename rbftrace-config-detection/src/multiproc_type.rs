use std::collections::HashSet;
use std::iter::FromIterator;

use rbftrace_core::sys_conf::{Pid, Cluster, MultiprocType, Cpu};

use crate::system::{get_rt_pids, all_cpu_mask_vec, filter_ht_pinned_kthreads, filter_unmovable_pinned_kthreads, get_affinity};

pub fn get_multiproc_type() -> MultiprocType {
    let rt_pids = filter_ht_pinned_kthreads(&get_rt_pids());
    if empty_mask_present(&rt_pids) {
        MultiprocType::ERROR
    }
    else if check_global(&rt_pids) {
        MultiprocType::GLOBAL
    } 
    else if check_partitioned(&rt_pids) {
        MultiprocType::PARTITIONED
    }
    else if check_clustered(&rt_pids, true).is_some() {
        MultiprocType::CLUSTERED
    }
    else if check_clustered(&rt_pids, false).is_some() {
        MultiprocType::CLUSTEREDNF
    }
    else {
        MultiprocType::APA
    }
}

/* Global contraints are slightly relaxed: unmovable pinned kthreads are not considered */
pub fn check_global(rt_pids: &[Pid]) -> bool {
    let all_cpu_vec = all_cpu_mask_vec();
    
    for pid in filter_unmovable_pinned_kthreads(rt_pids) {
        let affinity = get_affinity(&pid).unwrap();
        if affinity != all_cpu_vec {
            return false;
        }
    }

    true
}

pub fn check_partitioned(rt_pids: &[Pid]) -> bool {
    for pid in rt_pids {
        let affinity = get_affinity(pid).unwrap();

        if affinity.len() != 1 {
            return false;
        }
    }

    true
}

pub fn check_clustered(rt_pids: &[Pid], fixed_cluster_size: bool) -> Option<Vec<Cluster>> {
    let mut cluster_set: Vec<HashSet<Cpu>> = Vec::new();
    let mut ret: Vec<Cluster> = Vec::new();
    let rt_pids_filtered = filter_unmovable_pinned_kthreads(rt_pids);
    let cluster_size = get_affinity(&rt_pids_filtered[0]).unwrap().len();
    for pid in rt_pids_filtered {
        let affinity = get_affinity(&pid).unwrap();
        if fixed_cluster_size && affinity.len() != cluster_size {
            return None;
        }
        cluster_set.push(affinity.into_iter().collect());
    }
    cluster_set.dedup();

    /* All sets couples must be either disjointed or equal */
    for (cluster_idx, i) in cluster_set.iter().enumerate() {
        for j in &cluster_set {
            if !i.is_disjoint(j) && !i.symmetric_difference(j).collect::<HashSet<_>>().is_empty() {
                return None;
            }
        }
        ret.push(Cluster::new(cluster_idx as u32, Vec::from_iter(i.clone()), Vec::new()));
    }

    Some(ret)
}

pub fn empty_mask_present(rt_pids: &[Pid]) -> bool {
    for pid in rt_pids {
        let affinity = get_affinity(pid);
        if affinity.unwrap().is_empty() {
            eprintln!("WARNING: Process {} has an empty affinity mask! Please assign an affinity mask. \
            Did you disable hyperthreading without reassigning affinity masks afterwards?", pid);
            return true;
        } 
    }
    false
}
