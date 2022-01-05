use rbftrace_config_detection::detect::detect_sys_conf;

fn main() {
    let sys_conf = detect_sys_conf();

    println!("{:#?}", sys_conf);
} 
