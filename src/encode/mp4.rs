use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};

use anyhow::{Context, anyhow};
use eframe::wgpu;

use crate::model::{EXPORT_HEIGHT, EXPORT_WIDTH, FPS, Preset, frame_time, loop_frames};
use crate::render::{Renderer, Uniforms};

pub enum Mp4Progress {
    Frame { current: u32, total: u32 },
    Done,
    Failed(String),
}

pub fn export_async(
    device: wgpu::Device,
    queue: wgpu::Queue,
    preset: Preset,
    path: PathBuf,
) -> Receiver<Mp4Progress> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(
        move || match export_blocking(&device, &queue, &preset, &path, &sender) {
            Ok(()) => {
                let _ = sender.send(Mp4Progress::Done);
            }
            Err(error) => {
                let _ = sender.send(Mp4Progress::Failed(format!("{error:#}")));
            }
        },
    );
    receiver
}

fn export_blocking(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    preset: &Preset,
    path: &Path,
    progress: &mpsc::Sender<Mp4Progress>,
) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("criando {}", parent.display()))?;
    }

    let mut child = Command::new(ffmpeg_bin())
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{EXPORT_WIDTH}x{EXPORT_HEIGHT}"),
            "-r",
            &FPS.to_string(),
            "-i",
            "-",
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "20",
            "-maxrate",
            "5M",
            "-bufsize",
            "10M",
            "-pix_fmt",
            "yuv420p",
            "-profile:v",
            "high",
            "-level",
            "5.0",
            "-x264-params",
            "colorprim=bt709:transfer=bt709:colormatrix=bt709",
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-movflags",
            "+faststart",
        ])
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| {
            format!(
                "executando ffmpeg (instale com `brew install ffmpeg`) para {}",
                path.display()
            )
        })?;

    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(anyhow!("ffmpeg não abriu o stdin"));
    };

    let renderer = Renderer::new(device, queue, EXPORT_WIDTH, EXPORT_HEIGHT);
    renderer.update_text(&preset.text);
    let frames = loop_frames();
    let write_result = (|| -> anyhow::Result<()> {
        for frame in 0..frames {
            let uniforms = Uniforms::from_preset(
                preset,
                frame_time(frame, frames),
                EXPORT_WIDTH,
                EXPORT_HEIGHT,
            );
            renderer.render(preset.scene, &uniforms);
            let pixels = renderer.read_pixels()?;
            stdin
                .write_all(&pixels)
                .context("enviando frames para o ffmpeg")?;
            let _ = progress.send(Mp4Progress::Frame {
                current: frame + 1,
                total: frames,
            });
        }
        Ok(())
    })();
    drop(stdin);

    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }

    let output = child
        .wait_with_output()
        .context("aguardando o ffmpeg terminar")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ffmpeg terminou com {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    Ok(())
}

fn ffmpeg_bin() -> &'static str {
    // shortcut: caminhos fixos do Homebrew, usar configuração se o app for distribuído.
    for candidate in ["/opt/homebrew/bin/ffmpeg", "/usr/local/bin/ffmpeg"] {
        if Path::new(candidate).exists() {
            return candidate;
        }
    }
    "ffmpeg"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{GradientStyle, Preset};

    #[test]
    #[ignore = "requer GPU e ffmpeg"]
    fn export_mp4_smoke() {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            ..Default::default()
        }))
        .expect("adaptador wgpu");
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("mp4 test"),
            ..Default::default()
        }))
        .expect("device wgpu");

        let mut full = Preset::gradient(GradientStyle::Mesh);
        full.bars_enabled = true;
        full.warp.enabled = true;
        full.text.enabled = true;
        let cases = [
            ("target/shine-test.mp4", Preset::shine()),
            (
                "target/gradient-test.mp4",
                Preset::gradient(GradientStyle::Horizon),
            ),
            ("target/full-test.mp4", full),
        ];
        for (path, preset) in cases {
            let path = Path::new(path);
            let _ = std::fs::remove_file(path);
            let (sender, _receiver) = mpsc::channel();
            export_blocking(&device, &queue, &preset, path, &sender).expect("export mp4");

            let size = std::fs::metadata(path).expect("arquivo mp4").len();
            assert!(size > 100_000, "mp4 pequeno demais: {size} bytes");
        }
    }
}
