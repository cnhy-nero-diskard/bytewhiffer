#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod scanner;
mod theme;
mod treemap;
mod util;

use eframe::egui;

fn main() -> eframe::Result<()> {
    // Hidden dev flags: `bytewhiffer --debug-screenshot[-live|-drill]
    // <out.png> <path>` scans <path>, captures one frame at the chosen
    // moment (after completion / mid-scan / drilled into the largest
    // directory), and exits.
    let args: Vec<String> = std::env::args().collect();

    // Hidden dev flag: `bytewhiffer --debug-perf` runs the headless
    // soft-elevation tessellation spike (flat baseline vs shadow+gradient on a
    // synthetic dense tree) and exits without opening a window.
    if args.get(1).map(String::as_str) == Some("--debug-perf") {
        app::run_perf_bench();
        return Ok(());
    }

    let shot_mode = |flag: &str| match flag {
        "--debug-screenshot" => Some(app::DebugShotMode::Final),
        "--debug-screenshot-live" => Some(app::DebugShotMode::Live),
        "--debug-screenshot-drill" => Some(app::DebugShotMode::Drill),
        _ => None,
    };
    let debug_shot = (args.len() == 4)
        .then(|| shot_mode(&args[1]))
        .flatten()
        .map(|mode| {
            app::DebugShot::new(
                std::path::PathBuf::from(&args[2]),
                std::path::PathBuf::from(&args[3]),
                mode,
            )
        });

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Bytewhiffer")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([640.0, 400.0]),
        ..Default::default()
    };
    eframe::run_native(
        "bytewhiffer",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            let app = match debug_shot {
                Some(shot) => app::BytewhifferApp::with_debug_shot(shot),
                None => app::BytewhifferApp::default(),
            };
            Ok(Box::new(app))
        }),
    )
}
