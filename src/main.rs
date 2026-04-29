#![windows_subsystem = "windows"]

use anyhow::{Context, Result, bail};
use eframe::egui;
use nod::{
    common::Format,
    read::{DiscOptions, DiscReader, PartitionEncryption},
    write::{DiscWriter, FormatOptions, ProcessOptions, ScrubLevel},
};
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufWriter, Seek, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
};

fn load_icon() -> egui::IconData {
    let bytes = include_bytes!("../assets/icon.png");
    let img = image::load_from_memory(bytes).expect("invalid icon");
    let img = img.into_rgba8();
    let (w, h) = img.dimensions();
    egui::IconData {
        rgba: img.into_raw(),
        width: w,
        height: h,
    }
}

// Bundled at compile time — no download needed
const WIITDB: &str = include_str!("wiitdb.txt");

fn parse_wiitdb() -> HashMap<String, String> {
    WIITDB
        .lines()
        .filter_map(|line| {
            let (id, title) = line.split_once(" = ")?;
            let id = id.trim();
            if id == "TITLES" {
                return None; // header line
            }
            Some((id.to_string(), title.trim().to_string()))
        })
        .collect()
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RVZ2WBFS")
            .with_inner_size([560.0, 320.0])
            .with_resizable(false)
            .with_icon(load_icon()),
        ..Default::default()
    };
    eframe::run_native(
        "RVZ2WBFS",
        options,
        Box::new(|_cc| Ok(Box::new(App::new()))),
    )
}

// ── App ───────────────────────────────────────────────────────────────────────

#[derive(Default)]
struct App {
    wiitdb: HashMap<String, String>,
    input_path: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    state: AppState,
}

#[derive(Default, Clone)]
enum AppState {
    #[default]
    Idle,
    Converting(Arc<Mutex<ConversionProgress>>),
    Done { folder: PathBuf },
    Error(String),
}

#[derive(Default)]
struct ConversionProgress {
    message: String,
    percent: u64,
    finished: bool,
    result: Option<Result<PathBuf, String>>,
}

impl App {
    fn new() -> Self {
        Self {
            wiitdb: parse_wiitdb(),
            ..Default::default()
        }
    }
}

// ── UI ────────────────────────────────────────────────────────────────────────

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("RVZ2WBFS");
            ui.label("Converts Wii disc images to properly named WBFS folders for USB loading.");
            ui.separator();
            ui.add_space(10.0);

            // ── File pickers ────────────────────────────────────────────────
            ui.horizontal(|ui| {
                ui.label("Input: ");
                if ui.button("Browse…").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(
                            "Wii Disc Image",
                            &["rvz", "wia", "iso", "gcm", "wbfs", "ciso", "gcz", "tgc", "nfs"],
                        )
                        .pick_file()
                    {
                        self.input_path = Some(path);
                    }
                }
                let label = self
                    .input_path
                    .as_ref()
                    .map(|p| p.file_name().unwrap_or_default().to_string_lossy().into_owned())
                    .unwrap_or_else(|| "(none)".into());
                ui.label(label);
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.label("Output:");
                if ui.button("Browse…").clicked() {
                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
                        self.output_dir = Some(path);
                    }
                }
                let label = self
                    .output_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "(same folder as input)".into());
                ui.label(label);
            });

            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Output: <dir> / <Game Title> [GAMEID] / GAMEID.wbfs")
                    .italics()
                    .weak(),
            );

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(8.0);

            // ── Conversion ──────────────────────────────────────────────────
            match self.state.clone() {
                AppState::Idle => {
                    let ready = self.input_path.is_some();
                    ui.add_enabled_ui(ready, |ui| {
                        if ui.button("  Convert to WBFS  ").clicked() {
                            let input = self.input_path.clone().unwrap();
                            let out_dir = self.output_dir.clone().unwrap_or_else(|| {
                                input.parent().unwrap_or(Path::new(".")).to_path_buf()
                            });
                            let wiitdb = self.wiitdb.clone();
                            let progress = Arc::new(Mutex::new(ConversionProgress::default()));
                            let progress_clone = Arc::clone(&progress);
                            let ctx_clone = ctx.clone();

                            thread::spawn(move || {
                                let result = convert(&input, &out_dir, &wiitdb, |msg, pct| {
                                    if let Ok(mut p) = progress_clone.lock() {
                                        p.message = msg.to_string();
                                        p.percent = pct;
                                    }
                                    ctx_clone.request_repaint();
                                });
                                if let Ok(mut p) = progress_clone.lock() {
                                    p.finished = true;
                                    p.result = Some(result.map_err(|e| format!("{e:#}")));
                                }
                                ctx_clone.request_repaint();
                            });

                            self.state = AppState::Converting(progress);
                        }
                    });

                    if !ready {
                        ui.label("Select an input file to begin.");
                    }
                }

                AppState::Converting(progress) => {
                    let (msg, pct, finished, result) = {
                        let p = progress.lock().unwrap();
                        (p.message.clone(), p.percent, p.finished, p.result.clone())
                    };

                    if finished {
                        self.state = match result {
                            Some(Ok(folder)) => AppState::Done { folder },
                            Some(Err(e)) => AppState::Error(e),
                            None => AppState::Error("No result returned".into()),
                        };
                        ctx.request_repaint();
                        return;
                    }

                    ui.label(if msg.is_empty() { "Starting…" } else { &msg });
                    ui.add(egui::ProgressBar::new(pct as f32 / 100.0).show_percentage());
                    ctx.request_repaint();
                }

                AppState::Done { ref folder } => {
                    ui.label(
                        egui::RichText::new("✔ Conversion complete!")
                            .color(egui::Color32::GREEN)
                            .strong(),
                    );
                    ui.label(format!("Output: {}", folder.display()));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let f = folder.clone();
                        if ui.button("Open folder").clicked() {
                            let _ = open::that(f);
                        }
                        if ui.button("Convert another file").clicked() {
                            self.state = AppState::Idle;
                            self.input_path = None;
                        }
                    });
                }

                AppState::Error(ref msg) => {
                    ui.label(
                        egui::RichText::new(format!("Error: {msg}")).color(egui::Color32::RED),
                    );
                    ui.add_space(8.0);
                    if ui.button("Try again").clicked() {
                        self.state = AppState::Idle;
                    }
                }
            }
        });
    }
}

// ── Conversion logic ─────────────────────────────────────────────────────────

fn convert(
    input: &PathBuf,
    out_dir: &Path,
    wiitdb: &HashMap<String, String>,
    mut on_progress: impl FnMut(&str, u64),
) -> Result<PathBuf> {
    let cpus = num_cpus::get();
    let preloader_threads = match cpus {
        0..=4 => 1,
        5..=8 => 2,
        _ => 4,
    };
    let processor_threads = cpus.saturating_sub(preloader_threads).max(1);

    let disc_opts = DiscOptions {
        partition_encryption: PartitionEncryption::Original,
        preloader_threads,
    };

    on_progress("Opening disc image…", 0);
    let disc_reader = DiscReader::new(input, &disc_opts)?;
    let header = disc_reader.header();

    let game_id = header.game_id_str().to_string();
    let disc_title = header
        .game_title_str()
        .trim_matches('\0')
        .trim()
        .to_string();

    let display_title = wiitdb
        .get(&game_id)
        .cloned()
        .unwrap_or_else(|| disc_title.clone());

    let safe_title = sanitize_filename::sanitize(&display_title);
    let folder_name = format!("{safe_title} [{game_id}]");
    let game_dir = out_dir.join(&folder_name);
    let wbfs_path = game_dir.join(format!("{game_id}.wbfs"));

    if wbfs_path.exists() {
        bail!("Output already exists: {}", wbfs_path.display());
    }

    fs::create_dir_all(&game_dir)?;

    let out_opts = FormatOptions::new(Format::Wbfs);
    let process_opts = ProcessOptions {
        processor_threads,
        digest_crc32: true,
        digest_md5: false,
        digest_sha1: true,
        digest_xxh64: true,
        scrub: ScrubLevel::None,
    };

    let out_file = File::create(&wbfs_path)?;
    let mut out_writer = BufWriter::new(out_file);
    let disc_writer = DiscWriter::new(disc_reader, &out_opts)?;

    let mut prev_pct = 101u64;
    let finalization = disc_writer.process(
        |data, progress, total| {
            out_writer.write_all(&data)?;
            let pct = if total > 0 { progress * 100 / total } else { 0 };
            if pct != prev_pct {
                on_progress(&format!("Converting {display_title}…  {pct:02}%"), pct);
                prev_pct = pct;
            }
            Ok(())
        },
        &process_opts,
    )?;

    if !finalization.header.is_empty() {
        out_writer.rewind()?;
        out_writer.write_all(&finalization.header)?;
    }

    out_writer.flush()?;
    Ok(game_dir)
}
