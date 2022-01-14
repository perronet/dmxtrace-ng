use rbftrace_core::sys_conf::SysConf;
use rbftrace_core::model::SystemModel;
use rbftrace_model_extraction::{
    ModelExtractor, 
    IncrementalSystemModelExtractor, 
    ModelExtractionParameters
};
use structopt::StructOpt;
use serde_yaml;
use std::fs::{
    OpenOptions,
    remove_file,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::io::Write;

use rbftrace_core::time::*;
use rbftrace_core::model::{ScalarTaskModel};
use rbftrace_core::trace::{Trace};

fn main() {
    let args = Opt::from_args();

    // Check args
    if let Some(0) = args.update_arrival {
        panic!("Arrivals must be > 0");
    }

    let exit_code = match _main(args) {
        Ok(()) => 0,
        Err(AppError::TraceError(e)) => {
            eprintln!("Trace error: {:#?}", e);
            1
        },
        Err(AppError::OSError(e)) => {
            eprintln!("Bad input: {:#?}", e);
            2
        },
        Err(AppError::DeserializationFailure(e)) => {
            eprintln!("Cannot deserialize: {:#?}", e);
            3
        },
    };
    std::process::exit(exit_code);
}


fn _main(args: Opt) -> AppResult {
    let trace = Trace::from_yaml_file(&args.source_path)?;
    let extraction_params = ModelExtractionParameters::from(&args);
    let mut model = SystemModel::new(SysConf::default());
    let mut report: Vec<(u64, SystemModel)> = Vec::new();

    if args.update_interval.is_none() && args.update_arrival.is_none() {
        /* ONE-SHOT */
        let model_extractor = ModelExtractor::new(extraction_params, SysConf::default());
        model = model_extractor.extract_model(trace);

    } else {
        /* INCREMENTAL */
        let mut model_extractor = IncrementalSystemModelExtractor::new(extraction_params, SysConf::default());
        let mut last_update_time = Time::zero();
        let mut can_update = false;
        let mut arrival_cnt = 1; // So that we start at 2 samples
        let update_interval = Time::from_s(args.update_interval.unwrap() as f64);

        for event in trace.events() {
            /* Check if the model could have changed, perform model extraction only in that case */
            if model_extractor.push_event(event) {
                can_update = true;
                arrival_cnt += 1;
            }

            /* Perform model extraction every update_interval seconds or every update_arrival arrivals */
            if last_update_time.is_zero() {
                last_update_time = event.instant;
            }
            
            let last_update_elapsed = event.instant - last_update_time;

            if can_update && 
               (args.update_interval.is_some() && 
               last_update_elapsed >= update_interval ||
                args.update_arrival.is_some() && 
                arrival_cnt % args.update_arrival.unwrap() == 0) {
                
                model = model_extractor.extract_model();

                /* Print current models */
                if args.print {
                    print_models(&model.scalar_models);
                    println!("----------");
                }
                /* Add to report */
                if args.report {
                    report.push((arrival_cnt, model.clone()));
                }
                last_update_time = event.instant;
                can_update = false;
            }
        }

        /* Trace might have been shorter than update_interval or there might be events left */
        if can_update {
            model = model_extractor.extract_model();
            /* Add to report */
            if args.report {
                report.push((arrival_cnt, model.clone()));
            }
        }
    }

    /* Print final models */
    if args.print || args.output_path.is_none() {
        print_models(&model.scalar_models);
    }

    /* Convert to output format */
    let output = dd::Output::from(model);
    let output_report = dd::OutputReport::from(report);
    
    if let Some(ref path) = args.output_path {
        /* Write final model in output file */
        if path.exists() {
            remove_file(path)?;
        }
        let mut outputfile = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)?;

        let serialized;
        if args.report {
            serialized = serde_yaml::to_string(&output_report)?;
        } else {
            serialized = serde_yaml::to_string(&output)?;
        }
        write!(outputfile, "{}", serialized)?;
    }

    Ok(())
}

/* Args */
#[derive(Debug, StructOpt)]
pub struct Opt {
    /// Specify the event source (YAML file).
    #[structopt(short = "s", long)]
    pub source_path: String,

    /// Specify the output (YAML file).
    /// If not specified, will only print human readable output.
    #[structopt(short = "o", long, parse(from_os_str))]
    pub output_path: Option<PathBuf>,

    /// Perform model matching every "interval" seconds.
    /// 0 seconds means matching at each step of the model matcher.
    #[structopt(short = "i", long="interval")]
    pub update_interval: Option<f32>,

    /// Perform model matching every n arrivals.
    /// 1 arrival means matching at each step of the model matcher.
    #[structopt(short = "a", long="arrival")]
    pub update_arrival: Option<u64>,

    /// Output a file with the extracted model at each step.
    #[structopt(short = "r", long="report", requires("output-path"))]
    pub report: bool,

    /// Print extracted scalar models at each step.
    #[structopt(short = "p", long)]
    pub print: bool,

    // TUNABLES
    /// Jitter bound.
    #[structopt(short = "J", long, default_value="1500000")]
    pub jitter_bound: Jitter,

    /// Arrivals buffer size.
    #[structopt(short = "B", long, default_value="1000")]
    pub buf_size: usize,

    /// Maximal busy window for RBFs.
    #[structopt(short = "w", long, default_value="1000")]
    pub window_size: usize,
}

impl From<&Opt> for ModelExtractionParameters {
    fn from(opts: &Opt) -> Self {
        let mut ret = ModelExtractionParameters::default();

        ret.set_jmax(opts.jitter_bound);
        ret.set_rbf_window_size(opts.window_size);

        ret
    }
}

/* I/O formats and conversions */

mod dd {
    use std::collections::BTreeMap;
    use rbftrace_core::model::{SystemModel, ScalarTaskModel};
    use serde::{Deserialize, Serialize};

    use rbftrace_core::time::Pid;
    use rbftrace_core::rbf::Point;

    /* Note: we do not include the priority of the thread in the output.
       That information can be inferred from the system configuration. */
    #[derive(Serialize, Deserialize, Debug)]
    pub struct Output {
        // Human-readable
        pub scalar_models: BTreeMap<Pid, Option<ScalarTaskModel>>,
        // Not human-readable
        pub curve_models: BTreeMap<Pid, OutputRbf>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct OutputReport {
        pub run_models: Vec<(u64, BTreeMap<Pid, Option<ScalarTaskModel>>)>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct OutputRbf {
        pub rbf: Vec<Point>,
    }

    /* Only converting output */
    mod conversion {
        use std::convert::From;

        use super::*;
        use crate::dd;
        use rbftrace_core::rbf::RbfCurve;

        impl From<&RbfCurve> for dd::OutputRbf {
            fn from(rbf_curve: &RbfCurve) -> Self {
                let mut points: Vec<Point> = Vec::new();
                for p in rbf_curve.curve.into_iter() {
                    points.push(p);
                }

                dd::OutputRbf {
                    rbf: points,
                }
            }
        }
    }

    impl Output {
        pub fn new() -> Self {
            Output {
                scalar_models: BTreeMap::new(),
                curve_models: BTreeMap::new(),
            }
        }
    }

    impl OutputReport {
        pub fn new() -> Self {
            OutputReport {
                run_models: Vec::new(),
            }
        }
    }

    impl From<SystemModel> for Output {
        fn from(model: SystemModel) -> Self {
            let mut output = Output::new();

            for pid in model.pids() {
                let scalar_model = model.get_scalar_models(*pid);
                let m = scalar_model.and(Some(scalar_model.unwrap().clone()));

                output.scalar_models.insert(*pid, m);
                let output_rbf = OutputRbf::from(model.get_rbf(*pid).unwrap());
                output.curve_models.insert(*pid, output_rbf); 
            }

            output
        }
    }

    impl From<Vec<(u64, SystemModel)>> for OutputReport {
        fn from(model_report: Vec<(u64, SystemModel)>) -> Self {
            let mut output = OutputReport::new();

            for (samples, model) in model_report {
                let mut entry = (samples, BTreeMap::new());
                for pid in model.pids() {
                    let scalar_model = model.get_scalar_models(*pid);
                    let m = scalar_model.and(Some(scalar_model.unwrap().clone()));
    
                    entry.1.insert(*pid, m);
                }
                output.run_models.push(entry);
            }

            output
        }
    }
}

/* Error handling */

type AppResult = Result<(), AppError>;

enum AppError {
    TraceError(rbftrace_core::trace::TraceError),
    // MatcherError(),
    OSError(std::io::Error),
    DeserializationFailure(serde_yaml::Error),
}

impl From<rbftrace_core::trace::TraceError> for AppError {
    fn from(e: rbftrace_core::trace::TraceError) -> AppError {
        AppError::TraceError(e)
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> AppError {
        AppError::OSError(e)
    }
}

impl From<serde_yaml::Error> for AppError {
    fn from(e: serde_yaml::Error) -> AppError {
        AppError::DeserializationFailure(e)
    }
}

/* Support */

fn print_models(models: &BTreeMap<Pid, ScalarTaskModel>) {
    for (pid, model) in models {
        println!("PID {}:", pid);
        model.pretty_print();
    }
}
