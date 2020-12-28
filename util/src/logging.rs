pub fn logging_setup() {
    use log::info;
    use simplelog::*;
    use std::fs::OpenOptions;

    if let Ok(file) = OpenOptions::new().append(true).create(true).open("/tmp/plugin.log") {
        WriteLogger::init(
            LevelFilter::Info, Config::default(), file,
        ).unwrap();
        info!("{}", build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version, $.compiler, $.timestamp))
    }
}
