mod app;
mod encode;
mod model;
mod render;
mod text;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 900.0])
            .with_min_inner_size([980.0, 700.0])
            .with_title("BG Creator"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "BG Creator",
        options,
        Box::new(|cc| Ok(Box::new(app::BgCreatorApp::new(cc)))),
    )
}
