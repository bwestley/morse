#![windows_subsystem = "windows"]

use std::{
    fs,
    time::{Duration, SystemTime},
};

use egui::{
    remap_clamp, Button, Color32, ColorImage, DragValue, Pos2, Rect, RichText, TextureHandle, Vec2,
};
use screenshots::Screen;

mod morse_decoder;
use morse_decoder::*;
use serde::{Deserialize, Serialize};

fn get_max_size(size: Vec2, max_size: Vec2) -> Vec2 {
    let mut desired_size = size.clone();
    desired_size *= (max_size.x / desired_size.x).min(1.0);
    desired_size *= (max_size.y / desired_size.y).min(1.0);
    desired_size
}

#[derive(Serialize, Deserialize, Default)]
struct Config {
    sensor: SensorSettings,
    decoder: DecoderSettings,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct SensorSettings {
    on_color: (u8, u8, u8),
    off_color: (u8, u8, u8),
    on_threshold: f32,
}

impl Default for SensorSettings {
    fn default() -> Self {
        Self {
            on_color: (255, 255, 255),
            off_color: (255, 255, 255),
            on_threshold: 0.5,
        }
    }
}

fn lerp(x: f32, a: u8, b: u8) -> u8 {
    let min = a.min(b);
    let max = a.max(b);
    if x <= 0.0 {
        min
    } else if x >= 1.0 {
        max
    } else {
        ((max - min) as f32 * x) as u8 + min
    }
}

fn lerp3(x: f32, a: (u8, u8, u8), b: (u8, u8, u8)) -> (u8, u8, u8) {
    (lerp(x, a.0, b.0), lerp(x, a.1, b.1), lerp(x, a.2, b.2))
}

fn inverse_lerp(x: u8, a: u8, b: u8) -> f32 {
    let min = a.min(b);
    let max = a.max(b);
    if x <= min {
        0.0
    } else if x >= max {
        1.0
    } else {
        (x - min) as f32 / (max - min) as f32
    }
}

fn inverse_lerp3(x: (u8, u8, u8), a: (u8, u8, u8), b: (u8, u8, u8)) -> f32 {
    (inverse_lerp(x.0, a.0, b.0) + inverse_lerp(x.1, a.1, b.1) + inverse_lerp(x.2, a.2, b.2)) / 3.0
}

/// Get the path of the configuration file path.
/// [this executable's directory]/config.toml
fn get_config_file_path() -> Result<std::path::PathBuf, String> {
    match std::env::current_exe() {
        Err(exe_path_error) => {
            return Err(format!(
                "Unable to obtain executable directory: {exe_path_error}."
            ))
        }
        Ok(exe_path) => match exe_path.parent() {
            None => return Err("Unable to obtain executable directory.".to_string()),
            Some(parent_dir) => Ok(parent_dir.join("config.toml")),
        },
    }
}

/// Load the toml configuration from [`get_config_file_path`].
fn load_config() -> Result<Config, String> {
    let config_file_path = get_config_file_path()?;
    println!(
        "[Configuration Loader] Loading configuration file \"{}\".",
        config_file_path.display()
    );

    match fs::read_to_string(&config_file_path) {
        Ok(config_data) => match toml::from_str(&config_data) {
            Err(error) => {
                println!(
                    "[Configuration Loader] Unable to deserialize configuration file: {error}."
                );
                Err(format!(
                    "Unable to deserialize configuration file: {error}."
                ))
            }
            Ok(config) => Ok(config),
        },
        Err(read_error) => {
            println!("Unable to open configuration file: {read_error}. Installing default.");
            if let Err(write_error) = fs::write(
                &config_file_path,
                toml::to_string_pretty(&Config::default()).unwrap(),
            ) {
                println!("[Configuration Loader] Unable to install default configuration file: {write_error}.");
                return Err(format!(
                    "Unable to install default configuration file: {write_error}."
                ));
            }
            match fs::read_to_string(&config_file_path) {
                Err(read_error) => {
                    println!("[Configuration Loader] Unable to open newly created configuration file: {read_error}.");
                    return Err(format!(
                        "Unable to open newly created configuration file: {read_error}."
                    ));
                }
                Ok(serialized_config) => match toml::from_str(&serialized_config) {
                    Err(deserialize_error) => {
                        println!("[Configuration Loader] Unable to deserialize default configuration file: {deserialize_error}.");
                        Err(format!("Unable to deserialize default configuration file: {deserialize_error}."))
                    }
                    Ok(config) => Ok(config),
                },
            }
        }
    }
}

/// Save the toml configuration to [`get_config_file_path`].
/// Returns true if saved, false if not saved, or a string describing an error.
fn save_config(config: &Config) -> Result<bool, String> {
    match toml::to_string_pretty(config) {
        Err(error) => {
            println!("[Configuration Saver] Unable to serialize configuration file: {error}.");
            Err(format!("Unable to serialize configuration file: {error}."))
        }
        Ok(serialized_config) => {
            let config_file_path = get_config_file_path()?;
            println!(
                "[Configuration Saver] Saving configuration file \"{}\".",
                config_file_path.display()
            );
            match fs::write(&config_file_path, serialized_config) {
                Err(error) => {
                    println!("[Configuration Saver] Unable to write configuration file: {error}.");
                    Err(format!("Unable to write configuration file: {error}."))
                }
                Ok(_) => Ok(true),
            }
        }
    }
}

struct Morse {
    painter: egui::Painter,
    message: RichText,
    screens: Vec<Screen>,
    selected_screen: usize,
    last_time: SystemTime,
    preview: Option<(TextureHandle, Vec<u8>)>,
    frame_width: u32,
    frame_height: u32,
    sensor_position: (u32, u32, usize),
    sensor_settings: SensorSettings,
    decoder_settings: DecoderSettings,
    decoder: MorseDecoder,
    recording_window: bool,
    recording: bool,
}

impl Morse {
    const MAX_FRAME_DELAY: Duration = Duration::from_millis(20);

    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Load config
        let (m1, e1, sensor_settings, decoder_settings) = match load_config() {
            Ok(config) => (
                "Loaded config.toml.".to_owned(),
                false,
                config.sensor,
                config.decoder,
            ),
            Err(error) => (
                error,
                true,
                SensorSettings::default(),
                DecoderSettings::default(),
            ),
        };

        // Get screens
        let (m2, e2, screens) = match Screen::all() {
            Ok(screens) => (
                format!("\nFound {} screens.", screens.len()),
                false,
                screens,
            ),
            Err(error) => (
                format!("\nFailed to find screens: {error}."),
                true,
                Vec::new(),
            ),
        };

        // Compile message
        let message = RichText::new(m1 + &m2).color(if e1 || e2 {
            Color32::RED
        } else {
            Color32::GREEN
        });

        // Construct object
        Self {
            painter: cc.egui_ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("overlay"),
            )),
            message,
            screens,
            selected_screen: 9999,
            last_time: SystemTime::now(),
            preview: None,
            frame_width: 10,
            frame_height: 10,
            sensor_position: (0, 0, 0),
            sensor_settings,
            decoder_settings,
            decoder: MorseDecoder::new(),
            recording_window: false,
            recording: false,
        }
    }
}

impl eframe::App for Morse {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Calculate frame rate
        let now = SystemTime::now();
        let duration = now
            .duration_since(self.last_time)
            .unwrap_or_default()
            .as_millis();
        self.last_time = now;

        // Set ui style
        let mut style: egui::Style = (*ctx.style()).clone();
        style.override_text_style = Some(egui::TextStyle::Monospace);
        ctx.set_style(style);

        egui::CentralPanel::default().show(ctx, |ui| {
            // Display frame rate
            ui.label(format!(
                "{:04}ms ({:03.1}fps)",
                duration,
                1000.0 / duration as f32
            ));

            // Display message
            ui.label(self.message.clone());

            // Save config.toml
            if ui.button("Save config.toml").clicked() {
                if let Err(error) = save_config(&Config {
                    sensor: self.sensor_settings,
                    decoder: self.decoder_settings,
                }) {
                    self.message = RichText::new(error).color(Color32::RED);
                }
            }

            // Recognition settings
            ui.label(format!(
                "Sensor Position: ({}, {})",
                self.sensor_position.0, self.sensor_position.1
            ));

            if ui
                .add(
                    Button::new(RichText::new("Set On Color").color(Color32::from_rgb(
                        255 - self.sensor_settings.on_color.0,
                        255 - self.sensor_settings.on_color.1,
                        255 - self.sensor_settings.on_color.2,
                    )))
                    .fill(Color32::from_rgb(
                        self.sensor_settings.on_color.0,
                        self.sensor_settings.on_color.1,
                        self.sensor_settings.on_color.2,
                    )),
                )
                .clicked()
            {
                if let Some(preview) = &self.preview {
                    self.sensor_settings.on_color = (
                        preview.1[self.sensor_position.2],
                        preview.1[self.sensor_position.2 + 1],
                        preview.1[self.sensor_position.2 + 2],
                    );
                }
            }
            if ui
                .add(
                    Button::new(RichText::new("Set Off Color").color(Color32::from_rgb(
                        255 - self.sensor_settings.off_color.0,
                        255 - self.sensor_settings.off_color.1,
                        255 - self.sensor_settings.off_color.2,
                    )))
                    .fill(Color32::from_rgb(
                        self.sensor_settings.off_color.0,
                        self.sensor_settings.off_color.1,
                        self.sensor_settings.off_color.2,
                    )),
                )
                .clicked()
            {
                if let Some(preview) = &self.preview {
                    self.sensor_settings.off_color = (
                        preview.1[self.sensor_position.2],
                        preview.1[self.sensor_position.2 + 1],
                        preview.1[self.sensor_position.2 + 2],
                    );
                }
            }

            // Recording window
            if ui.button("Recording").clicked() {
                self.recording_window = true;
            }

            let mut recording_window = self.recording_window;
            egui::Window::new("Recording")
                .open(&mut recording_window)
                .show(ctx, |ui| {
                    // Start/stop recording
                    if ui
                        .button(if self.recording {
                            "Stop Recording"
                        } else {
                            "Start Recording"
                        })
                        .clicked()
                    {
                        self.recording ^= true;
                    }

                    // Reset
                    if ui.button("Reset").clicked() {
                        self.decoder.reset();
                    }

                    // Sensor
                    if self.recording {
                        if let Some(screen) = self.screens.get(self.selected_screen) {
                            match screen.capture_area(
                                self.sensor_position.0.try_into().unwrap(),
                                self.sensor_position.1.try_into().unwrap(),
                                1,
                                1,
                            ) {
                                Ok(image) => {
                                    let rgba = image.rgba();
                                    let rgb = (rgba[0], rgba[1], rgba[2]);
                                    let threshold_color = lerp3(
                                        self.sensor_settings.on_threshold,
                                        self.sensor_settings.off_color,
                                        self.sensor_settings.on_color,
                                    );

                                    let (response, painter) = ui.allocate_painter(
                                        Vec2::new(150.0, 100.0),
                                        egui::Sense::hover(),
                                    );
                                    let x = response.rect.min.x;
                                    let y = response.rect.min.y;

                                    // Sensor color
                                    painter.rect_filled(
                                        Rect::from_min_size(
                                            Pos2::new(x, y),
                                            Vec2::new(150.0, 50.0),
                                        ),
                                        0.0,
                                        Color32::from_rgb(rgb.0, rgb.1, rgb.2),
                                    );

                                    // Off Color
                                    painter.rect_filled(
                                        Rect::from_min_size(
                                            Pos2::new(x, y + 50.0),
                                            Vec2::new(50.0, 50.0),
                                        ),
                                        0.0,
                                        Color32::from_rgb(
                                            self.sensor_settings.off_color.0,
                                            self.sensor_settings.off_color.1,
                                            self.sensor_settings.off_color.2,
                                        ),
                                    );

                                    // Threshold Color
                                    painter.rect_filled(
                                        Rect::from_min_size(
                                            Pos2::new(x + 50.0, y + 50.0),
                                            Vec2::new(50.0, 50.0),
                                        ),
                                        0.0,
                                        Color32::from_rgb(
                                            threshold_color.0,
                                            threshold_color.1,
                                            threshold_color.2,
                                        ),
                                    );

                                    // On Color
                                    painter.rect_filled(
                                        Rect::from_min_size(
                                            Pos2::new(x + 100.0, y + 50.0),
                                            Vec2::new(50.0, 50.0),
                                        ),
                                        0.0,
                                        Color32::from_rgb(
                                            self.sensor_settings.on_color.0,
                                            self.sensor_settings.on_color.1,
                                            self.sensor_settings.on_color.2,
                                        ),
                                    );

                                    // Threshold
                                    let f = inverse_lerp3(
                                        rgb,
                                        self.sensor_settings.off_color,
                                        self.sensor_settings.on_color,
                                    );
                                    painter.line_segment(
                                        [
                                            Pos2::new(
                                                x + 150.0 * self.sensor_settings.on_threshold,
                                                y,
                                            ),
                                            Pos2::new(
                                                x + 150.0 * self.sensor_settings.on_threshold,
                                                y + 100.0,
                                            ),
                                        ],
                                        egui::Stroke::new(5.0, Color32::GRAY),
                                    );
                                    painter.line_segment(
                                        [
                                            Pos2::new(x + 150.0 * f, y),
                                            Pos2::new(x + 150.0 * f, y + 100.0),
                                        ],
                                        egui::Stroke::new(
                                            5.0,
                                            if f < self.sensor_settings.on_threshold {
                                                Color32::RED
                                            } else {
                                                Color32::GREEN
                                            },
                                        ),
                                    );

                                    self.decoder.tick(f >= self.sensor_settings.on_threshold);
                                }
                                Err(error) => {
                                    self.message =
                                        RichText::new(format!("Error capturing screen: {error}."))
                                            .monospace()
                                            .color(Color32::RED);
                                }
                            }
                        }
                    }

                    // Display code
                    ui.label(Code::display_code_string(
                        self.decoder.decode(&self.decoder_settings),
                    ));

                    // Decoder settings
                    ui.add(
                        egui::Slider::new(&mut self.sensor_settings.on_threshold, 0.0..=1.0)
                            .text("On Threshold"),
                    );
                    egui::Grid::new("decoder settings").show(ui, |ui| {
                        ui.label("Dit/Dah Threshold (ms)");
                        ui.add(DragValue::new(&mut self.decoder_settings.dit_dah));
                        ui.end_row();
                        ui.label("Minimum Letter Gap (ms)");
                        ui.add(DragValue::new(&mut self.decoder_settings.letter));
                        ui.end_row();
                        ui.label("Minimum Word Gap (ms)");
                        ui.add(DragValue::new(&mut self.decoder_settings.letter_word));
                    });

                    // Display recorded timings
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.label(self.decoder.display());
                    });
                });
            self.recording_window = recording_window;

            // Screen selection
            ui.label("Screen Selection:");
            ui.radio_value(&mut self.selected_screen, 9999, "None");
            for i in 0..self.screens.len() {
                if ui
                    .radio_value(
                        &mut self.selected_screen,
                        i,
                        self.screens[i].display_info.id.to_string(),
                    )
                    .clicked()
                {
                    self.preview = None;
                    self.sensor_position = (0, 0, 0);
                }
            }

            if ui.button("Update Preview").clicked() {
                // Capture screen
                if let Some(screen) = self.screens.get(self.selected_screen) {
                    match screen.capture() {
                        Ok(image) => {
                            self.frame_width = image.width();
                            self.frame_height = image.height();

                            self.preview = Some((
                                ctx.load_texture(
                                    "preview",
                                    ColorImage::from_rgba_unmultiplied(
                                        [
                                            self.frame_width.try_into().unwrap(),
                                            self.frame_height.try_into().unwrap(),
                                        ],
                                        image.rgba(),
                                    ),
                                    egui::TextureOptions::LINEAR,
                                ),
                                image.rgba().clone(),
                            ));
                        }
                        Err(error) => {
                            self.message =
                                RichText::new(format!("Error capturing screen: {error}."))
                                    .monospace()
                                    .color(Color32::RED);
                        }
                    };
                };
            }

            if let Some(preview) = &mut self.preview {
                // Display the preview
                let preview_response = ui
                    .image(
                        preview.0.id(),
                        get_max_size(
                            Vec2::new(self.frame_width as f32, self.frame_height as f32),
                            ui.available_size(),
                        ),
                    )
                    .interact(egui::Sense::click());

                // Draw sensor circle
                self.painter.circle_stroke(
                    Pos2::new(
                        remap_clamp(
                            self.sensor_position.0 as f32,
                            0.0..=self.frame_width as f32,
                            preview_response.rect.min.x..=preview_response.rect.max.x,
                        ),
                        remap_clamp(
                            self.sensor_position.1 as f32,
                            0.0..=self.frame_height as f32,
                            preview_response.rect.min.y..=preview_response.rect.max.y,
                        ),
                    ),
                    10.0,
                    egui::Stroke::new(2.0, Color32::GREEN),
                );

                // Preview interaction
                if preview_response.clicked() {
                    if let Some(screen_position) = preview_response.interact_pointer_pos() {
                        let x = remap_clamp(
                            screen_position.x,
                            preview_response.rect.min.x..=preview_response.rect.max.x,
                            0.0..=self.frame_width as f32,
                        )
                        .floor() as u32;
                        let y = remap_clamp(
                            screen_position.y,
                            preview_response.rect.min.y..=preview_response.rect.max.y,
                            0.0..=self.frame_height as f32,
                        )
                        .floor() as u32;
                        let i = (y as usize * self.frame_width as usize + x as usize) * 4;
                        self.sensor_position = (x, y, i);
                    }
                }
            }
        });

        ctx.request_repaint_after(Self::MAX_FRAME_DELAY);
    }
}

fn main() {
    let mut native_options = eframe::NativeOptions::default();
    native_options.min_window_size = Some(Vec2::new(850.0, 500.0));
    let _ = eframe::run_native(
        "Morse",
        native_options,
        Box::new(|cc| Box::new(Morse::new(cc))),
    );
}
