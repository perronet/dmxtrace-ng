use std::path::{PathBuf, Path};

use rbftrace_core::{
    model::{SystemModel, PeriodicTask}, 
    sys_conf::{SysConf},
    trace::Trace, 
    time::{Time, Jitter}
};
use rbftrace_model_extraction::{
    periodic::{PeriodicTaskExtractionParams},
    rbf::{RBFExtractionParams}, 
    SystemModelExtractor, 
    composite::{CompositeExtractionParams, CompositeModelExtractor, CompositeModel},
};

use dd::WriteYAML;
use structopt::StructOpt;

fn create_dir<P: AsRef<Path>>(output_dir: P) -> Result<(), AppError> {
    std::fs::create_dir_all(output_dir).map_err(|e| AppError::OSError(e)) 
}

fn print_periodic_models(system_model: &SystemModel<CompositeModel>) {
    for pid in system_model.pids() {
        println!("PID {}:", pid);
        let model = system_model.get_model(*pid)
                                    .map(|m| m.periodic)
                                    .flatten();

        if let Some(model) = model {
            model.pretty_print();
        } else {
            println!("Not periodic");
        }
    }
}

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
    let extraction_params = CompositeExtractionParams::from(&args);
    let mut model = SystemModel::new(SysConf::default());
    // let mut report: Vec<(u64, SystemModel<CompositeModel>)> = Vec::new();
    let mut report = dd::Report::<PeriodicTask>::new();


    if args.update_interval.is_none() && args.update_arrival.is_none() {
        if args.report {
            eprintln!("Option --report set for a one shot extraction. Report won't be written");
        }
        /* ONE-SHOT */
        model = SystemModelExtractor::<CompositeModelExtractor>::extract_from_trace(extraction_params, SysConf::default(), trace);
    } else {
        /* INCREMENTAL */
        let mut model_extractor = SystemModelExtractor::<CompositeModelExtractor>::new(extraction_params, SysConf::default());

        let mut last_update_time = Time::zero();
        let mut model_changed = false;
        let mut arrival_cnt = 1; // So that we start at 2 samples
        let update_interval = Time::from_s(args.update_interval.unwrap() as f64);

        for event in trace.events() {
            /* Check if the model could have changed, perform model extraction only in that case */
            model_changed = model_extractor.push_event(*event);

            if model_changed {
                arrival_cnt += 1;
            }

            /* Perform model extraction every update_interval seconds or every update_arrival arrivals */
            if last_update_time.is_zero() {
                last_update_time = event.instant;
            }
            
            let last_update_elapsed = event.instant - last_update_time;

            if model_changed && 
               (args.update_interval.is_some() && 
               last_update_elapsed >= update_interval ||
                args.update_arrival.is_some() && 
                arrival_cnt % args.update_arrival.unwrap() == 0) {
                
                model = model_extractor.extract_model();

                /* Print current models */
                if args.print {
                    print_periodic_models(&model);
                    println!("----------");
                }
                /* Add to report */
                if args.report {
                    report.push_model(arrival_cnt as usize, &model);
                }

                last_update_time = event.instant;
                model_changed = false;
            }
        }

        /* Trace might have been shorter than update_interval or there might be events left */
        if model_changed {
            model = model_extractor.extract_model();
            /* Add to report */
            if args.report {
                report.push_model(arrival_cnt as usize, &model);
            }
        }
    }

    /* Print final models */
    if args.print || args.output_path.is_none() {
        print_periodic_models(&model);
    }

    if let Some(ref path) = args.output_path {
        create_dir(path)?;

        if args.report {
            report.write_yaml(path)?;
        } else {
            dd::Output::from(&model).write_yaml(path)?;
        }
    }

    Ok(())
}

/* Args */
#[derive(Debug, StructOpt)]
pub struct Opt {
    /// Specify the event source (YAML file).
    #[structopt(short = "s", long)]
    pub source_path: String,

    /// Specify the output directory.
    /// If not specified, will only print human readable output.
    /// The directory must not exist.
    /// Matched model are written in output_path/[pid].[model].yaml
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
    /// Reports are written in output_path/[pid].[model].report.yaml
    #[structopt(long="report", requires("output-path"))]
    pub report: bool,

    /// Print extracted scalar models at each step.
    #[structopt(short = "p", long)]
    pub print: bool,

    // TUNABLES
    /// Jitter bound.
    #[structopt(short = "J", long="J_max", default_value="1500000")]
    pub jitter_bound: Jitter,

    /// Maximal busy window for RBFs.
    #[structopt(short = "w", long, default_value="1000")]
    pub window_size: usize,

    #[structopt(short= "r", long="resolution", default_value="100000")]
    pub resolution:Time,
}

impl From<&Opt> for CompositeExtractionParams {
    fn from(opts: &Opt) -> Self {
        let periodic = PeriodicTaskExtractionParams {
            j_max: opts.jitter_bound,
            resolution: opts.resolution,
        };

        let rbf = RBFExtractionParams {
            window_size: opts.window_size
        };

        CompositeExtractionParams {
            periodic,
            rbf
        }
    }
}
/* I/O formats and conversions */
mod dd {
    use std::{collections::BTreeMap, path::Path, fs::OpenOptions};
    use rbftrace_core::{model::{SystemModel, PeriodicTask}, rbf::RbfCurve};
    use rbftrace_model_extraction::composite::CompositeModel;
    use serde::{Deserialize, Serialize, Serializer};

    use rbftrace_core::{sys_conf::Pid, rbf::Point};

    use crate::AppError;
    
    pub trait WriteYAML {
        fn write_yaml<P: AsRef<Path>>(&self, output_dir: P) -> Result<(), AppError>;
    }
    
    /* Note: we do not include the priority of the thread in the output.
       That information can be inferred from the system configuration. */
    #[derive(Serialize, Deserialize, Debug)]
    pub struct Output {
        // Human-readable
        pub scalar_models: BTreeMap<Pid, Option<PeriodicTask>>,
        // Not human-readable
        pub curve_models: BTreeMap<Pid, OutputRbf>,
    }
    
    impl Output {
        pub fn new() -> Self {
            Output {
                scalar_models: BTreeMap::new(),
                curve_models: BTreeMap::new(),
            }
        }
    }
    
    impl From<&SystemModel<CompositeModel>> for Output {
        fn from(model: &SystemModel<CompositeModel>) -> Self {
            let mut output = Output::new();

            for pid in model.pids() {
                let model = model.get_model(*pid);

                if let Some(model) = model {
                    let periodic = model.periodic;
                    output.scalar_models.insert(*pid, periodic);

                    let output_rbf = OutputRbf::from(&model.rbf);
                    output.curve_models.insert(*pid, output_rbf);
                }
            }

            output
        }
    }
    
    impl WriteYAML for Output {
        fn write_yaml<P: AsRef<Path>>(&self, output_dir: P) -> Result<(), AppError> {
            for (pid, model) in &self.scalar_models {
                if let Some(model) = model {
                    let filename = format!("{}.periodic.yaml", pid);
                    let path = Path::new(output_dir.as_ref()).join(filename);

                    let file = OpenOptions::new().create_new(true)
                                                      .write(true)
                                                      .open(path)
                                                      .map_err(|err| AppError::OSError(err))?;

                    serde_yaml::to_writer(file, &model).map_err(|e|AppError::DeserializationFailure(e))?;
                }

            }
            
            for (pid, model) in &self.curve_models {
                let filename = format!("{}.rbf.yaml", pid);
                let path = Path::new(output_dir.as_ref()).join(filename);

                let file = OpenOptions::new().create_new(true)
                                                  .write(true)
                                                  .open(path)
                                                  .map_err(|err| AppError::OSError(err))?;

                serde_yaml::to_writer(file, &model).map_err(|e|AppError::DeserializationFailure(e))?;
            }

            Ok(())   
        }
    }
    #[derive(Serialize, Deserialize, Debug)]
    pub struct OutputRbf {
        pub rbf: Vec<Point>,
    }
    
    impl From<&RbfCurve> for OutputRbf {
        fn from(rbf_curve: &RbfCurve) -> Self {
            let mut points: Vec<Point> = Vec::new();
            for p in rbf_curve.curve.into_iter() {
                points.push(p);
            }

            OutputRbf {
                rbf: points,
            }
        }
    }
    
    // serialization function for Option<Model>, returns not matched as a default value
    fn serialize_matched_model<S, T>(model: &Option<T>,s: S) -> Result<S::Ok, S::Error> 
    where S: Serializer, T: Serialize{
        if model.is_none() {
            s.serialize_str("Not matched")
        } else {
            model.serialize(s)
        }
    }
    #[derive(Serialize, Debug)]
    struct ReportEntry<T: Serialize > {
        sample_count: usize,
        #[serde(serialize_with="serialize_matched_model")]
        model: Option<T>
    }

    pub struct Report<T: Serialize> {
        entries: BTreeMap<Pid, Vec<ReportEntry<T>>>
    }

    impl Report<PeriodicTask> {
        pub fn new() -> Self {
            Self {
                entries: BTreeMap::new()
            }
        }

        pub fn push_model(&mut self, count: usize, model: &SystemModel<CompositeModel>) {
            
            for pid in model.pids() {
                let m = model.get_model(*pid)
                                               .map(|e| e.periodic)
                                               .flatten();
                let record_entry = ReportEntry{
                    sample_count: count,
                    model: m
                };

                let e: &mut Vec<ReportEntry<PeriodicTask>> = self.entries
                                                             .entry(*pid)
                                                             .or_insert_with(|| vec![]);
                
                e.push(record_entry);       
            }
        }
    }

    impl WriteYAML for Report<PeriodicTask> {
        fn write_yaml<P: AsRef<Path>>(&self, output_dir: P) -> Result<(), AppError>{
            for (pid, model) in &self.entries {
                let filename = format!("{}.periodic.report.yaml", pid);
                let path = Path::new(output_dir.as_ref()).join(filename);

                let file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .map_err(|err| AppError::OSError(err))?;

                serde_yaml::to_writer(file, &model).map_err(|e|AppError::DeserializationFailure(e))?;
            }

            Ok(())
        }
    }
}
/* Error handling */

type AppResult = Result<(), AppError>;

pub enum AppError {
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
