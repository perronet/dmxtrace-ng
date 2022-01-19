use structopt::StructOpt;
use std::fs::{
    OpenOptions,
    remove_file,
};
use std::path::PathBuf;
use std::io::Write;

use rbftrace_core::time::*;
use rbftrace_core::trace::{
    TraceEvent,
};
use rbftrace_tracing::ftrace::FTraceEVG;
use rbftrace_config_detection::system::get_pids_with_policy;
use rbftrace_core::sys_conf::{SchedPolicy, Pid};

fn main() {
    let args = Opt::from_args();
    let traced_pids: Vec<Pid>;
    let target_pids: Vec<Pid>;
    let mut output: Vec<TraceEvent> = Vec::new();

    /* Parsing */
    if let Some(pids) = args.pids {
        traced_pids = pids;
    } else {
        traced_pids = get_pids_with_policy(vec!(SchedPolicy::FIFO, SchedPolicy::RR), false);
    }

    if let Some(ref pids) = args.target_pids {
        target_pids = pids.clone();
    } else {
        target_pids = traced_pids.clone();
    }

    let mut outputfile = None;
    if let Some(ref path) = args.output {
        if path.exists() {
            remove_file(path).unwrap();
        }
        let file = OpenOptions::new()
        .create_new(true)
        .append(true)
        .open(path)
        .expect("Can't initialize file.");
        outputfile = Some(file);
    }

    /* Tracing */
    let mut evg = FTraceEVG::new(&target_pids, &traced_pids, Time::from_s(args.ftrace_len).to_ns(), args.ftrace_bufsize);
    
    evg.setup();

    println!("Traced pids: {:#?}", traced_pids);
    if let Some(ref target_pids) = args.target_pids {
        println!("Target pids: {:#?}", target_pids);
    }
    
    while let Some(event) = evg.next_event() {
        if outputfile.is_none() {
            let serialized = serde_yaml::to_string(&event).expect("Can't serialize.");
            print!("{}", serialized);
        } else {
            output.push(event);
        }
    }

    if let Some(ref mut file) = outputfile {
        let serialized = serde_yaml::to_string(&output).expect("Can't serialize.");
        write!(file, "{}", serialized).expect("I/O error.");
    }
}

#[derive(Debug, StructOpt)]
pub struct Opt {
    /// Trace only the specified pids. By default, all real-time threads are traced.
    #[structopt(short = "p", long)]
    pub pids: Option<Vec<Pid>>,

    /// Trace until the specified pids are dead. By default, tracing is done until all traced pids are dead, unless -l is specified.
    #[structopt(short = "t", long)]
    pub target_pids: Option<Vec<Pid>>,

    /// Trace for the specified duration (in seconds).
    #[structopt(short = "l", long, default_value = "0")]
    pub ftrace_len: f64,

    /// Set ftrace buffer size (in kb).
    #[structopt(short = "b", long, default_value = "65536")]
    pub ftrace_bufsize: u32,

    /// Output file, stdout if not present.
    #[structopt(short = "o", long, parse(from_os_str))]
    pub output: Option<PathBuf>,
}
