use log::LevelFilter;

pub fn init_log(level_filter: LevelFilter) {
    let noise_level = if level_filter < LevelFilter::Warn {
        level_filter
    } else {
        LevelFilter::Warn
    };
    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{}[{}][{}] {}",
                chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
                record.target(),
                record.level(),
                message
            ))
        })
        .level(level_filter)
        .level_for("reqwest", noise_level)
        .level_for("want", noise_level)
        .level_for("mio", noise_level)
        .chain(std::io::stdout())
        // .chain(fern::log_file("output.log")?)
        .apply()
        .unwrap();
}
