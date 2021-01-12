#[cfg(not(feature="enable_logging"))]
pub fn logging_setup() {}


#[cfg(feature="enable_logging")]
pub fn logging_setup() {
    use log::info;
    use simplelog::*;
    use std::fs::OpenOptions;

    unsafe { if LOGGING_SETUP { return } }

    if let Ok(file) = OpenOptions::new().append(true).create(true).open("/tmp/plugin.log") {
        let mut config = ConfigBuilder::new();
        config.set_time_format("%+".to_string());
        WriteLogger::init(
            LevelFilter::Info, config.build(), file,
        ).unwrap();
        info!("{}", build_info::format!("{{{} v{} built with {} at {}}}", $.crate_info.name, $.crate_info.version, $
        .compiler, $.timestamp))
    }

    log_panics::init();

    unsafe { LOGGING_SETUP = true }
}

static mut LOGGING_SETUP: bool = false;
