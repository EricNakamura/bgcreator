use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use eframe::egui;

use crate::encode::mp4::Mp4Progress;
use crate::model::{
    ColorGrade, DEFAULT_PRESET_PATH, EXPORT_HEIGHT, EXPORT_WIDTH, FPS, GradientStyle, LOOP_SECONDS,
    Palette, Preset, SceneId, TextConfig, TextDirection, TextLine, TextMotion, WarpMap, hex_to_rgb,
    loop_frames,
};
use crate::render::{Renderer, Uniforms};

pub struct BgCreatorApp {
    preset: Preset,
    renderer: Renderer,
    texture_id: egui::TextureId,
    playing: bool,
    clock: f32,
    last_tick: Instant,
    show_lyrics: bool,
    show_safe_zone: bool,
    status: String,
    preset_path: PathBuf,
    rendered_text: TextConfig,
    export_rx: Option<Receiver<Mp4Progress>>,
    export_progress: Option<(u32, u32)>,
}

impl BgCreatorApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let render_state = cc
            .wgpu_render_state
            .as_ref()
            .expect("o backend wgpu é obrigatório");
        let (renderer, texture_id) = Renderer::from_render_state(render_state, 960, 600);

        let preset_path = PathBuf::from(DEFAULT_PRESET_PATH);
        let (preset, status) = load_preset(SceneId::Shine, &preset_path);
        renderer.update_text(&preset.text);
        let rendered_text = preset.text.clone();

        Self {
            preset,
            renderer,
            texture_id,
            playing: true,
            clock: 0.0,
            last_tick: Instant::now(),
            show_lyrics: true,
            show_safe_zone: true,
            status,
            preset_path,
            rendered_text,
            export_rx: None,
            export_progress: None,
        }
    }

    fn switch_scene(&mut self, scene: SceneId) {
        if self.preset.scene == scene {
            return;
        }
        let path = PathBuf::from(format!("presets/{}.toml", scene.slug()));
        let (preset, status) = load_preset(scene, &path);
        self.preset = preset;
        self.preset_path = path;
        self.status = status;
        self.renderer.update_text(&self.preset.text);
        self.rendered_text = self.preset.text.clone();
        self.clock = 0.0;
        self.playing = true;
    }

    fn export_png(&mut self) {
        let path = unique_export_path(PathBuf::from(format!(
            "exports/{}.png",
            self.preset.file_stem()
        )));
        let time = self.clock / LOOP_SECONDS;
        match self.renderer.export_png(&self.preset, time, &path) {
            Ok(()) => self.status = format!("PNG exportado: {}", path.display()),
            Err(error) => self.status = format!("Falha ao exportar: {error:#}"),
        }
    }

    fn save_preset(&mut self) {
        match self.preset.save(&self.preset_path) {
            Ok(()) => self.status = format!("Preset salvo: {}", self.preset_path.display()),
            Err(error) => self.status = format!("Falha ao salvar: {error:#}"),
        }
    }

    fn export_mp4(&mut self) {
        if self.export_rx.is_some() {
            return;
        }
        let path = unique_export_path(PathBuf::from(format!(
            "exports/{}.mp4",
            self.preset.file_stem()
        )));
        self.export_progress = Some((0, loop_frames()));
        self.status = format!("Exportando MP4: {}", path.display());
        self.export_rx = Some(self.renderer.export_mp4_async(self.preset.clone(), path));
    }

    fn poll_export(&mut self) {
        let Some(receiver) = self.export_rx.take() else {
            return;
        };
        let mut finished = false;
        loop {
            match receiver.try_recv() {
                Ok(Mp4Progress::Frame { current, total }) => {
                    self.export_progress = Some((current, total));
                }
                Ok(Mp4Progress::Done) => {
                    self.status = "MP4 exportado. Importe no Holyrics e ative o loop.".into();
                    self.export_progress = None;
                    finished = true;
                    break;
                }
                Ok(Mp4Progress::Failed(error)) => {
                    self.status = format!("Falha ao exportar MP4: {error}");
                    self.export_progress = None;
                    finished = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.status = "Exportação interrompida.".into();
                    self.export_progress = None;
                    finished = true;
                    break;
                }
            }
        }
        if !finished {
            self.export_rx = Some(receiver);
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("BG Creator");
        ui.label("Fundos procedurais para o Holyrics");

        let current_scene = self.preset.scene;
        let mut chosen_scene = None;
        egui::ComboBox::from_label("Cena")
            .selected_text(current_scene.label())
            .show_ui(ui, |ui| {
                for scene in SceneId::ALL {
                    if ui
                        .selectable_label(current_scene == scene, scene.label())
                        .clicked()
                    {
                        chosen_scene = Some(scene);
                    }
                }
            });
        if let Some(scene) = chosen_scene {
            self.switch_scene(scene);
        }

        if self.preset.scene == SceneId::Gradient {
            let current_style = self.preset.gradient_style;
            let mut chosen_style: Option<GradientStyle> = None;
            egui::ComboBox::from_label("Estilo")
                .selected_text(current_style.label())
                .show_ui(ui, |ui| {
                    for style in GradientStyle::ALL {
                        if ui
                            .selectable_label(current_style == style, style.label())
                            .clicked()
                        {
                            chosen_style = Some(style);
                        }
                    }
                });
            if let Some(style) = chosen_style {
                self.preset.gradient_style = style;
                self.preset.name = format!("Gradient {}", style.label());
            }
        }

        ui.label(format!(
            "Cena: {} · {}×{} · {} fps · {} frames",
            self.preset.scene.label(),
            EXPORT_WIDTH,
            EXPORT_HEIGHT,
            FPS,
            loop_frames()
        ));
        ui.separator();

        let current_palette = self.preset.palette.name.clone();
        let mut chosen_palette: Option<Palette> = None;
        egui::ComboBox::from_label("Paleta")
            .selected_text(&current_palette)
            .show_ui(ui, |ui| {
                for palette in Palette::all() {
                    if ui
                        .selectable_label(current_palette == palette.name, &palette.name)
                        .clicked()
                    {
                        chosen_palette = Some(palette);
                    }
                }
            });
        if let Some(palette) = chosen_palette {
            self.preset.palette = palette;
        }

        ui.horizontal(|ui| {
            for hex in &self.preset.palette.colors {
                if let Some([r, g, b]) = hex_to_rgb(hex) {
                    let color = egui::Color32::from_rgb(
                        (r * 255.0).round() as u8,
                        (g * 255.0).round() as u8,
                        (b * 255.0).round() as u8,
                    );
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(22.0, 22.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 4.0, color);
                }
            }
        });

        ui.add(egui::Slider::new(&mut self.preset.speed, 0.0..=1.5).text("velocidade"));
        ui.add(egui::Slider::new(&mut self.preset.scale, 0.5..=1.8).text("escala"));
        ui.add(egui::Slider::new(&mut self.preset.intensity, 0.2..=2.0).text("intensidade"));
        ui.add(egui::Slider::new(&mut self.preset.vignette, 0.0..=1.0).text("vinheta"));
        ui.add(egui::Slider::new(&mut self.preset.center_dim, 0.0..=1.0).text("escurecer centro"));

        ui.separator();
        self.bars_controls(ui);
        self.warp_controls(ui);
        self.text_controls(ui);

        egui::CollapsingHeader::new("Correção de cor")
            .default_open(true)
            .show(ui, |ui| {
                ui.add(
                    egui::Slider::new(&mut self.preset.grade.exposure, -2.0..=2.0)
                        .text("exposição (EV)"),
                );
                ui.add(
                    egui::Slider::new(&mut self.preset.grade.brightness, -0.5..=0.5).text("brilho"),
                );
                ui.add(
                    egui::Slider::new(&mut self.preset.grade.contrast, -1.0..=1.0)
                        .text("contraste"),
                );
                ui.add(
                    egui::Slider::new(&mut self.preset.grade.saturation, 0.0..=2.0)
                        .text("saturação"),
                );
                ui.add(
                    egui::Slider::new(&mut self.preset.grade.temperature, -1.0..=1.0)
                        .text("temperatura"),
                );
                ui.add(
                    egui::Slider::new(&mut self.preset.grade.tint, -1.0..=1.0)
                        .text("tint (verde/magenta)"),
                );
                ui.add(egui::Slider::new(&mut self.preset.grade.gamma, 0.5..=2.0).text("gamma"));
                ui.add(
                    egui::Slider::new(&mut self.preset.grade.sharpness, 0.0..=1.0).text("nitidez"),
                );
                if ui.button("Resetar cor").clicked() {
                    self.preset.grade = ColorGrade::default();
                }
                ui.label(
                    egui::RichText::new("Aplica no preview e nos exports desta cena.")
                        .small()
                        .weak(),
                );
            });

        ui.horizontal(|ui| {
            ui.add(egui::Slider::new(&mut self.preset.seed, 0.0..=100.0).text("seed"));
            if ui.button("Aleatorizar").clicked() {
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                self.preset.seed = (nanos % 10_000) as f32 / 100.0;
            }
        });

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button(if self.playing { "Pausar" } else { "Reproduzir" })
                .clicked()
            {
                self.playing = !self.playing;
            }
            let mut seconds = self.clock;
            if ui
                .add(egui::Slider::new(&mut seconds, 0.0..=LOOP_SECONDS).text("tempo"))
                .changed()
            {
                self.clock = seconds;
                self.playing = false;
            }
        });
        ui.checkbox(&mut self.show_lyrics, "Mostrar letra de exemplo");
        ui.checkbox(&mut self.show_safe_zone, "Mostrar zona segura");

        ui.separator();
        if ui.button("Exportar PNG").clicked() {
            self.export_png();
        }
        let exporting = self.export_rx.is_some();
        if ui
            .add_enabled(!exporting, egui::Button::new("Exportar MP4 (12 s)"))
            .clicked()
        {
            self.export_mp4();
        }
        if let Some((current, total)) = self.export_progress {
            let fraction = current as f32 / total.max(1) as f32;
            ui.add(
                egui::ProgressBar::new(fraction)
                    .show_percentage()
                    .text(format!("{current}/{total} frames")),
            );
        }
        if ui.button("Salvar preset").clicked() {
            self.save_preset();
        }

        ui.separator();
        ui.label(egui::RichText::new(&self.status).small());
        ui.label(
            egui::RichText::new("No Holyrics: Tema → Plano de Fundo → adicionar imagem.")
                .small()
                .weak(),
        );
    }

    fn bars_controls(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Barras (4)")
            .default_open(false)
            .show(ui, |ui| {
                let available = self.preset.scene == SceneId::Gradient;
                ui.add_enabled_ui(available, |ui| {
                    ui.checkbox(&mut self.preset.bars_enabled, "Ativar 4 barras");
                });
                if !available {
                    ui.label(
                        egui::RichText::new("Disponível apenas na cena Gradient.")
                            .small()
                            .weak(),
                    );
                    return;
                }
                if !self.preset.bars_enabled {
                    return;
                }

                ui.label(
                    egui::RichText::new(
                        "Com barras ativas, o estilo e a paleta globais são ignorados.",
                    )
                    .small()
                    .weak(),
                );
                ui.add(egui::Slider::new(&mut self.preset.bar_gap, 0.0..=0.2).text("gap"));
                ui.add(
                    egui::Slider::new(&mut self.preset.bar_softness, 0.001..=0.1).text("suavidade"),
                );

                for index in 0..4 {
                    let bar = &mut self.preset.bars[index];
                    egui::CollapsingHeader::new(format!("Barra {}", index + 1))
                        .default_open(index == 0)
                        .show(ui, |ui| {
                            ui.checkbox(&mut bar.enabled, "ativa");
                            style_combo(ui, &format!("bar_style_{index}"), &mut bar.style);
                            palette_combo(ui, &format!("bar_palette_{index}"), &mut bar.palette);
                            ui.add(egui::Slider::new(&mut bar.phase, 0.0..=1.0).text("defasagem"));
                            ui.checkbox(&mut bar.mirrored, "espelhar");
                        });
                }
            });
    }

    fn warp_controls(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Warp global")
            .default_open(false)
            .show(ui, |ui| {
                ui.checkbox(&mut self.preset.warp.enabled, "Ativar warp");
                if !self.preset.warp.enabled {
                    return;
                }
                warp_map_combo(ui, "warp_map", &mut self.preset.warp.map);
                ui.add(egui::Slider::new(&mut self.preset.warp.strength, 0.0..=0.3).text("força"));
                ui.add(egui::Slider::new(&mut self.preset.warp.scale, 0.2..=3.0).text("escala"));
                ui.add(
                    egui::Slider::new(&mut self.preset.warp.speed, 0.0..=2.0).text("velocidade"),
                );
                ui.add(egui::Slider::new(&mut self.preset.warp.seed, 0.0..=100.0).text("seed"));
                ui.label(
                    egui::RichText::new("Aplica em cima de barras e texto.")
                        .small()
                        .weak(),
                );
            });
    }

    fn text_controls(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("Texto")
            .default_open(false)
            .show(ui, |ui| {
                ui.checkbox(&mut self.preset.text.enabled, "Ativar texto");
                if !self.preset.text.enabled {
                    return;
                }
                ui.label(
                    egui::RichText::new("Texto assado no vídeo. Para letras, use o Holyrics.")
                        .small()
                        .weak(),
                );

                let mut remove = None;
                for index in 0..self.preset.text.lines.len() {
                    let line = &mut self.preset.text.lines[index];
                    egui::CollapsingHeader::new(format!("Linha {}", index + 1))
                        .default_open(index == 0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut line.text)
                                    .desired_width(f32::INFINITY),
                            );
                            text_direction_combo(
                                ui,
                                &format!("text_direction_{index}"),
                                &mut line.direction,
                            );
                            text_motion_combo(
                                ui,
                                &format!("text_motion_{index}"),
                                &mut line.motion,
                            );
                            ui.add(
                                egui::Slider::new(&mut line.speed, 1.0..=4.0)
                                    .step_by(1.0)
                                    .text("velocidade"),
                            );
                            ui.add(egui::Slider::new(&mut line.size, 20.0..=300.0).text("tamanho"));
                            color_edit(ui, &mut line.color);
                            ui.add(
                                egui::Slider::new(&mut line.opacity, 0.0..=1.0).text("opacidade"),
                            );
                            ui.add(egui::Slider::new(&mut line.y, -1.0..=1.0).text("posição Y"));
                            if ui.button("Remover linha").clicked() {
                                remove = Some(index);
                            }
                        });
                }
                if let Some(index) = remove {
                    self.preset.text.lines.remove(index);
                }
                if self.preset.text.lines.len() < 4 && ui.button("Adicionar linha").clicked() {
                    let index = self.preset.text.lines.len();
                    self.preset.text.lines.push(TextLine::default_for(index));
                }
            });
    }

    fn preview(&mut self, ui: &mut egui::Ui) {
        let mut available = ui.available_size();
        available.y = (available.y - 34.0).max(1.0);
        let aspect = 16.0 / 10.0;
        let mut size = available;
        if size.x / size.y > aspect {
            size.x = size.y * aspect;
        } else {
            size.y = size.x / aspect;
        }

        let available_rect = ui.available_rect_before_wrap();
        let rect = egui::Rect::from_center_size(available_rect.center(), size);
        ui.allocate_rect(rect, egui::Sense::hover());
        ui.painter().image(
            self.texture_id,
            rect,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );

        if self.show_safe_zone {
            let safe = egui::Rect::from_center_size(
                rect.center(),
                egui::vec2(rect.width() * 0.8, rect.height() * 0.55),
            );
            ui.painter().rect_stroke(
                safe,
                4.0,
                egui::Stroke::new(1.0, egui::Color32::from_white_alpha(90)),
                egui::StrokeKind::Inside,
            );
        }

        if self.show_lyrics {
            let text = "Senhor, tu és a luz do mundo";
            let font = egui::FontId::proportional((rect.height() * 0.06).max(14.0));
            let pos = rect.center() + egui::vec2(0.0, rect.height() * 0.1);
            let outline = egui::Color32::from_black_alpha(220);
            for offset in [
                egui::vec2(-2.0, 0.0),
                egui::vec2(2.0, 0.0),
                egui::vec2(0.0, -2.0),
                egui::vec2(0.0, 2.0),
                egui::vec2(1.5, 1.5),
                egui::vec2(-1.5, 1.5),
                egui::vec2(1.5, -1.5),
                egui::vec2(-1.5, -1.5),
            ] {
                ui.painter().text(
                    pos + offset,
                    egui::Align2::CENTER_CENTER,
                    text,
                    font.clone(),
                    outline,
                );
            }
            ui.painter().text(
                pos,
                egui::Align2::CENTER_CENTER,
                text,
                font,
                egui::Color32::WHITE,
            );
        }

        ui.add_space(34.0);
        ui.label(
            egui::RichText::new(format!(
                "Preview {}×{} · export {}×{}",
                self.renderer.width(),
                self.renderer.height(),
                EXPORT_WIDTH,
                EXPORT_HEIGHT
            ))
            .small()
            .weak(),
        );
    }
}

impl eframe::App for BgCreatorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_export();
        let now = Instant::now();
        let delta = now.duration_since(self.last_tick).as_secs_f32().min(0.1);
        self.last_tick = now;
        if self.playing {
            self.clock = (self.clock + delta) % LOOP_SECONDS;
            ui.ctx().request_repaint();
        }
        if self.export_rx.is_some() {
            ui.ctx().request_repaint();
        }
        if text_needs_raster(&self.rendered_text, &self.preset.text) {
            self.renderer.update_text(&self.preset.text);
            self.rendered_text = self.preset.text.clone();
        }

        let time = self.clock / LOOP_SECONDS;
        let uniforms = Uniforms::from_preset(
            &self.preset,
            time,
            self.renderer.width(),
            self.renderer.height(),
        );
        self.renderer.render(self.preset.scene, &uniforms);

        egui::Panel::left("controls")
            .default_size(320.0)
            .resizable(false)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });
        egui::CentralPanel::default().show(ui, |ui| self.preview(ui));
    }
}

fn load_preset(scene: SceneId, path: &Path) -> (Preset, String) {
    match Preset::load(path) {
        Ok(preset) => (preset, format!("Preset: {}", path.display())),
        Err(error) if path.exists() => (
            Preset::for_scene(scene),
            format!("Preset inválido, usando padrão: {error:#}"),
        ),
        Err(_) => {
            let preset = Preset::for_scene(scene);
            let status = match preset.save(path) {
                Ok(()) => format!("Preset: {}", path.display()),
                Err(error) => format!("Não foi possível gravar o preset: {error:#}"),
            };
            (preset, status)
        }
    }
}

fn unique_export_path(base: PathBuf) -> PathBuf {
    if !base.exists() {
        return base;
    }
    let stem = base
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("export");
    let extension = base
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("bin");
    for index in 2..=999 {
        let candidate = base.with_file_name(format!("{stem}-{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    base
}

fn style_combo(ui: &mut egui::Ui, id: &str, style: &mut GradientStyle) {
    let current = *style;
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(current.label())
        .show_ui(ui, |ui| {
            for candidate in GradientStyle::ALL {
                if ui
                    .selectable_label(current == candidate, candidate.label())
                    .clicked()
                {
                    chosen = Some(candidate);
                }
            }
        });
    if let Some(candidate) = chosen {
        *style = candidate;
    }
}

fn palette_combo(ui: &mut egui::Ui, id: &str, palette: &mut Palette) {
    let current = palette.name.clone();
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(&current)
        .show_ui(ui, |ui| {
            for candidate in Palette::all() {
                if ui
                    .selectable_label(current == candidate.name, &candidate.name)
                    .clicked()
                {
                    chosen = Some(candidate);
                }
            }
        });
    if let Some(candidate) = chosen {
        *palette = candidate;
    }
}

fn warp_map_combo(ui: &mut egui::Ui, id: &str, map: &mut WarpMap) {
    let current = *map;
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(current.label())
        .show_ui(ui, |ui| {
            for candidate in WarpMap::ALL {
                if ui
                    .selectable_label(current == candidate, candidate.label())
                    .clicked()
                {
                    chosen = Some(candidate);
                }
            }
        });
    if let Some(candidate) = chosen {
        *map = candidate;
    }
}

fn text_direction_combo(ui: &mut egui::Ui, id: &str, direction: &mut TextDirection) {
    let current = *direction;
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(current.label())
        .show_ui(ui, |ui| {
            for candidate in [TextDirection::Left, TextDirection::Right] {
                if ui
                    .selectable_label(current == candidate, candidate.label())
                    .clicked()
                {
                    chosen = Some(candidate);
                }
            }
        });
    if let Some(candidate) = chosen {
        *direction = candidate;
    }
}

fn text_motion_combo(ui: &mut egui::Ui, id: &str, motion: &mut TextMotion) {
    let current = *motion;
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id)
        .selected_text(current.label())
        .show_ui(ui, |ui| {
            for candidate in TextMotion::ALL {
                if ui
                    .selectable_label(current == candidate, candidate.label())
                    .clicked()
                {
                    chosen = Some(candidate);
                }
            }
        });
    if let Some(candidate) = chosen {
        *motion = candidate;
    }
}

fn color_edit(ui: &mut egui::Ui, hex: &mut String) {
    let [r, g, b] = hex_to_rgb(hex).unwrap_or([1.0, 1.0, 1.0]);
    let mut rgb = [
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    ];
    if ui.color_edit_button_srgb(&mut rgb).changed() {
        *hex = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    }
}

fn text_needs_raster(previous: &TextConfig, current: &TextConfig) -> bool {
    previous.enabled != current.enabled
        || previous.lines.len() != current.lines.len()
        || previous
            .lines
            .iter()
            .zip(&current.lines)
            .any(|(old, new)| old.text != new.text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_paths_do_not_overwrite() {
        let dir = std::env::temp_dir().join(format!("bgcreator-paths-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("x.png");
        let second = dir.join("x-2.png");
        let _ = std::fs::remove_file(&base);
        let _ = std::fs::remove_file(&second);

        assert_eq!(unique_export_path(base.clone()), base);
        std::fs::write(&base, b"x").unwrap();
        assert_eq!(unique_export_path(base.clone()), second);
        std::fs::write(&second, b"x").unwrap();
        assert_eq!(unique_export_path(base.clone()), dir.join("x-3.png"));

        let _ = std::fs::remove_file(&base);
        let _ = std::fs::remove_file(&second);
        let _ = std::fs::remove_file(dir.join("x-3.png"));
    }

    #[test]
    fn only_text_changes_rerasterize() {
        let previous = TextConfig::default();
        let mut color_only = previous.clone();
        color_only.lines[0].color = "#ff0000".into();
        assert!(!text_needs_raster(&previous, &color_only));

        let mut changed = previous.clone();
        changed.lines[0].text = "Outro".into();
        assert!(text_needs_raster(&previous, &changed));
    }
}
