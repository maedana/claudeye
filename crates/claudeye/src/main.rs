mod crmux_client;
mod cursor;
mod picker;

use clap::{Parser, Subcommand};
use eframe::egui::{self, Color32, RichText, Ui, Vec2};
use std::sync::{Arc, Mutex};
use tmux_claude_state::claude_state::ClaudeState;
use tmux_claude_state::monitor::{ClaudeSession, MonitorState, start_polling};

#[derive(Parser)]
#[command(about = "Claude session monitor overlay", version)]
struct Args {
    /// Show state summary with session counts
    #[arg(long)]
    compact: bool,

    /// Overlay window position on screen
    #[arg(long, short, default_value = "top-center", value_enum)]
    position: Position,

    /// Move overlay to screen center when any session stays in Approval/Idle for 5-15 seconds
    #[arg(long)]
    alert_on_stale: bool,

    /// Use crmux socket for session data instead of direct tmux polling
    #[arg(long)]
    crmux: bool,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive TUI session picker
    Picker,
}

#[derive(Clone, Copy, Default, clap::ValueEnum)]
enum Position {
    TopLeft,
    #[default]
    TopCenter,
    TopRight,
    MiddleLeft,
    MiddleCenter,
    MiddleRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Position {
    fn compute(self, monitor: Vec2, window: Vec2) -> egui::Pos2 {
        let x = match self {
            Position::TopLeft | Position::MiddleLeft | Position::BottomLeft => MARGIN,
            Position::TopCenter | Position::MiddleCenter | Position::BottomCenter => {
                (monitor.x - window.x) / 2.0
            }
            Position::TopRight | Position::MiddleRight | Position::BottomRight => {
                monitor.x - window.x - MARGIN
            }
        };
        let y = match self {
            Position::TopLeft | Position::TopCenter | Position::TopRight => MARGIN,
            Position::MiddleLeft | Position::MiddleCenter | Position::MiddleRight => {
                (monitor.y - window.y) / 2.0
            }
            Position::BottomLeft | Position::BottomCenter | Position::BottomRight => {
                monitor.y - window.y - MARGIN
            }
        };
        egui::pos2(x, y)
    }
}

/// Try to load a CJK font from the system for Japanese/Chinese/Korean text rendering.
fn load_cjk_font() -> Option<egui::FontData> {
    use font_kit::family_name::FamilyName;
    use font_kit::properties::Properties;
    use font_kit::source::SystemSource;

    let families = [
        // Linux
        "Noto Sans CJK JP",
        "Noto Sans JP",
        // macOS
        "Hiragino Sans",
        "Hiragino Kaku Gothic ProN",
        // Windows
        "Yu Gothic",
        "MS Gothic",
    ];

    let source = SystemSource::new();
    for name in &families {
        if let Ok(handle) =
            source.select_best_match(&[FamilyName::Title(name.to_string())], &Properties::new())
        {
            if let Ok(font) = handle.load() {
                if let Some(data) = font.copy_font_data() {
                    return Some(egui::FontData::from_owned((*data).clone()));
                }
            }
        }
    }
    None
}

const REPAINT_INTERVAL_SECS: u64 = 2;
const STALE_MIN_SECS: u64 = 5;
const STALE_MAX_SECS: u64 = 15;
const MIN_WINDOW_WIDTH: f32 = 180.0;
const WINDOW_EMPTY_HEIGHT: f32 = 40.0;
const ROW_HEIGHT: f32 = 22.0;
const WINDOW_PADDING: f32 = 8.0;
const MARGIN: f32 = 2.0;
/// Horizontal overhead per session row (panel margin + robot art + spacing + bubble padding + buffer).
const ROW_HORIZONTAL_OVERHEAD: f32 = 82.0;
/// Base color for the robot body art.
const ROBOT_BODY_COLOR: Color32 = Color32::from_rgb(210, 110, 30);
/// Opacity when cursor hovers over the overlay (0.0 = invisible, 1.0 = fully opaque).
const HOVER_OPACITY: f32 = 0.0;
/// Lerp factor per frame for hover opacity animation (higher = faster).
const HOVER_LERP_FACTOR: f32 = 0.25;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    match args.command {
        Some(Commands::Picker) => picker::run_picker()?,
        None => run_gui(args.compact, args.position, args.alert_on_stale, args.crmux)?,
    }
    Ok(())
}

/// Data from crmux socket for a single session.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct CrmuxSession {
    pub pane_id: String,
    pub pid: u32,
    pub project_name: String,
    pub state: String,
    pub elapsed_secs: u64,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub context_percent: Option<u32>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub git_branch: Option<String>,
}

/// State shared between the crmux polling thread and the GUI.
#[derive(Clone, Debug, Default)]
pub struct CrmuxState {
    pub sessions: Vec<CrmuxSession>,
    pub visible: bool,
}

/// Format elapsed seconds as a human-readable duration (e.g. "45s", "2m", "1h 5m").
fn format_elapsed(secs: u64) -> String {
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m {s}s")
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h {m}m")
        }
    }
}

/// Convert a crmux state string to ClaudeState.
fn parse_crmux_state(s: &str) -> ClaudeState {
    match s {
        "Working" => ClaudeState::Working,
        "WaitingForApproval" | "Approval" => ClaudeState::WaitingForApproval,
        _ => ClaudeState::Idle,
    }
}

/// Format a crmux session label for rich display.
/// Example: "myproject (main) Opus 23% [Running] 45s"
pub fn format_crmux_label(session: &CrmuxSession, state_label: &str) -> String {
    let mut parts = Vec::new();

    // project_name (git_branch)
    if let Some(ref branch) = session.git_branch {
        parts.push(format!("{} ({})", session.project_name, branch));
    } else {
        parts.push(session.project_name.clone());
    }

    // title (strip newlines, truncate to 20 chars)
    if let Some(ref title) = session.title {
        let sanitized: String = title.chars().map(|c| if c == '\n' { ' ' } else { c }).collect();
        let truncated = if sanitized.chars().count() > 20 {
            let s: String = sanitized.chars().take(20).collect();
            format!("{s}…")
        } else {
            sanitized
        };
        if !truncated.is_empty() {
            parts.push(truncated);
        }
    }

    // model
    if let Some(ref model) = session.model {
        parts.push(model.clone());
    }

    // context_percent
    if let Some(pct) = session.context_percent {
        parts.push(format!("{}%", pct));
    }

    // [state] elapsed
    parts.push(format!("[{}] {}", state_label, format_elapsed(session.elapsed_secs)));

    parts.join("  ")
}

/// Convert CrmuxSession list to ClaudeSession list for rendering.
fn crmux_to_claude_sessions(crmux_sessions: &[CrmuxSession]) -> Vec<ClaudeSession> {
    use tmux_claude_state::tmux::PaneInfo;
    crmux_sessions
        .iter()
        .map(|cs| {
            let state = parse_crmux_state(&cs.state);
            ClaudeSession {
                pane: PaneInfo {
                    id: cs.pane_id.clone(),
                    pid: cs.pid,
                    cwd: String::new(),
                    project_name: cs.project_name.clone(),
                },
                state,
                state_changed_at: std::time::Instant::now()
                    - std::time::Duration::from_secs(cs.elapsed_secs),
            }
        })
        .collect()
}

/// Start a background thread that polls crmux socket every 2 seconds.
fn start_crmux_polling(crmux_state: Arc<Mutex<CrmuxState>>) {
    std::thread::spawn(move || loop {
        match crmux_client::fetch_sessions() {
            Ok(result) => {
                let visible = result
                    .get("visible")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(true);
                let sessions: Vec<CrmuxSession> = result
                    .get("sessions")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();

                if let Ok(mut state) = crmux_state.lock() {
                    state.sessions = sessions;
                    state.visible = visible;
                }
            }
            Err(_) => {
                if let Ok(mut state) = crmux_state.lock() {
                    state.sessions.clear();
                    state.visible = false;
                }
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(REPAINT_INTERVAL_SECS));
    });
}

fn run_gui(
    compact: bool,
    position: Position,
    alert_on_stale: bool,
    crmux: bool,
) -> eframe::Result<()> {
    let state: Arc<Mutex<MonitorState>> = Arc::new(Mutex::new(MonitorState::default()));
    let crmux_state: Arc<Mutex<CrmuxState>> = Arc::new(Mutex::new(CrmuxState::default()));

    if crmux {
        start_crmux_polling(Arc::clone(&crmux_state));
    } else {
        start_polling(Arc::clone(&state));
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_inner_size([MIN_WINDOW_WIDTH, WINDOW_EMPTY_HEIGHT])
            .with_transparent(true)
            .with_active(false),
        ..Default::default()
    };

    eframe::run_native(
        "claudeye",
        options,
        Box::new(|cc| {
            if let Some(cjk_font) = load_cjk_font() {
                let mut fonts = egui::FontDefinitions::default();
                fonts
                    .font_data
                    .insert("cjk_font".to_owned(), cjk_font.into());
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .push("cjk_font".to_owned());
                cc.egui_ctx.set_fonts(fonts);
            }

            let mut visuals = cc.egui_ctx.style().visuals.clone();
            visuals.panel_fill = Color32::TRANSPARENT;
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(CcMonitorApp {
                state,
                crmux_state,
                crmux,
                compact,
                position,
                alert_on_stale,
                hover_opacity: 1.0,
            }))
        }),
    )
}

struct CcMonitorApp {
    state: Arc<Mutex<MonitorState>>,
    crmux_state: Arc<Mutex<CrmuxState>>,
    crmux: bool,
    compact: bool,
    position: Position,
    alert_on_stale: bool,
    hover_opacity: f32,
}

impl eframe::App for CcMonitorApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
            egui::WindowLevel::AlwaysOnTop,
        ));
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));

        let (sessions, quiet, crmux_sessions) = if self.crmux {
            match self.crmux_state.lock() {
                Ok(guard) => {
                    if !guard.visible {
                        ctx.request_repaint_after(std::time::Duration::from_secs(
                            REPAINT_INTERVAL_SECS,
                        ));
                        return;
                    }
                    let cs = guard.sessions.clone();
                    (crmux_to_claude_sessions(&cs), false, Some(cs))
                }
                Err(_) => return,
            }
        } else {
            match self.state.lock() {
                Ok(guard) => (guard.sessions.clone(), guard.any_claude_focused, None),
                Err(_) => return, // poisoned mutex: polling thread panicked
            }
        };

        let needs_fast_repaint = !quiet
            && sessions.iter().any(|s| match s.state {
                ClaudeState::Working => !self.compact,
                ClaudeState::WaitingForApproval => true,
                ClaudeState::Idle => (STALE_MIN_SECS..=STALE_MAX_SECS)
                    .contains(&s.state_changed_at.elapsed().as_secs()),
            });
        if needs_fast_repaint {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        } else if !sessions.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        } else {
            ctx.request_repaint_after(std::time::Duration::from_secs(REPAINT_INTERVAL_SECS));
        }

        let time = ctx.input(|i| i.time);

        if self.compact {
            let summaries = state_summary(&sessions);

            let window_height = ROW_HEIGHT + WINDOW_PADDING * 2.0;
            let window_width = if summaries.is_empty() {
                MIN_WINDOW_WIDTH
            } else {
                let n = summaries.len() as f32;
                (n * COMPACT_GROUP_WIDTH + (n - 1.0) * COMPACT_GROUP_SPACING).max(MIN_WINDOW_WIDTH)
            };

            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                window_width,
                window_height,
            )));

            if let Some(monitor_size) = ctx.input(|i| i.viewport().monitor_size) {
                let effective_position =
                    if self.alert_on_stale && !quiet && should_alert_on_stale(&sessions) {
                        Position::MiddleCenter
                    } else {
                        self.position
                    };
                let pos = effective_position
                    .compute(monitor_size, Vec2::new(window_width, window_height));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            }

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::none()
                        .fill(Color32::TRANSPARENT)
                        .inner_margin(egui::Margin::symmetric(8.0, WINDOW_PADDING)),
                )
                .show(ctx, |ui| {
                    if summaries.is_empty() {
                        ui.label(
                            RichText::new("No Claude sessions found")
                                .color(Color32::from_gray(120))
                                .size(12.0),
                        );
                    } else {
                        let stale_idle = !quiet
                            && sessions.iter().any(|s| {
                                matches!(s.state, ClaudeState::Idle)
                                    && (STALE_MIN_SECS..=STALE_MAX_SECS)
                                        .contains(&s.state_changed_at.elapsed().as_secs())
                            });
                        render_compact_row(ui, &summaries, time, stale_idle);
                    }
                });
        } else {
            let display_sessions: Vec<&ClaudeSession> = sessions.iter().collect();

            let n = display_sessions.len() as f32;
            let window_height = if display_sessions.is_empty() {
                WINDOW_EMPTY_HEIGHT
            } else {
                // ROW_HEIGHT per row + 4px item_spacing between rows + top/bottom padding
                n * ROW_HEIGHT + (n - 1.0) * 4.0 + WINDOW_PADDING * 2.0
            };

            let window_width = if display_sessions.is_empty() {
                MIN_WINDOW_WIDTH
            } else if let Some(ref cs) = crmux_sessions {
                let max_text = cs
                    .iter()
                    .map(|s| measure_crmux_text_width(ctx, s))
                    .fold(0.0_f32, f32::max);
                (max_text + ROW_HORIZONTAL_OVERHEAD).max(MIN_WINDOW_WIDTH)
            } else {
                let max_text = display_sessions
                    .iter()
                    .map(|s| measure_session_text_width(ctx, s))
                    .fold(0.0_f32, f32::max);
                (max_text + ROW_HORIZONTAL_OVERHEAD).max(MIN_WINDOW_WIDTH)
            };

            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(
                window_width,
                window_height,
            )));

            let mut is_hovering = false;
            if let Some(monitor_size) = ctx.input(|i| i.viewport().monitor_size) {
                let effective_position =
                    if self.alert_on_stale && !quiet && should_alert_on_stale(&sessions) {
                        Position::MiddleCenter
                    } else {
                        self.position
                    };
                let pos = effective_position
                    .compute(monitor_size, Vec2::new(window_width, window_height));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));

                is_hovering = cursor::get_cursor_screen_position()
                    .map(|(cx, cy)| {
                        cursor::is_cursor_in_rect(
                            cx,
                            cy,
                            pos.x,
                            pos.y,
                            window_width,
                            window_height,
                        )
                    })
                    .unwrap_or(false);
            }

            let target_opacity = if is_hovering { HOVER_OPACITY } else { 1.0 };
            self.hover_opacity =
                cursor::lerp_opacity(self.hover_opacity, target_opacity, HOVER_LERP_FACTOR);

            let animating =
                (self.hover_opacity - target_opacity).abs() > cursor::OPACITY_SNAP_THRESHOLD;
            if animating || is_hovering {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }

            let hover_opacity = self.hover_opacity;

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::none()
                        .fill(Color32::TRANSPARENT)
                        .inner_margin(egui::Margin::symmetric(8.0, WINDOW_PADDING)),
                )
                .show(ctx, |ui| {
                    if display_sessions.is_empty() {
                        ui.label(
                            RichText::new("No Claude sessions found")
                                .color(apply_opacity(Color32::from_gray(120), hover_opacity))
                                .size(12.0),
                        );
                    } else if let Some(ref cs) = crmux_sessions {
                        for (session, crmux_s) in display_sessions.iter().zip(cs.iter()) {
                            render_crmux_session_row(
                                ui,
                                session,
                                crmux_s,
                                time,
                                hover_opacity,
                            );
                        }
                    } else {
                        for session in &display_sessions {
                            render_session_row(ui, session, time, quiet, hover_opacity);
                        }
                    }
                });
        }
    }
}

/// Measure the rendered text width of a session row using the egui font system.
///
/// State label is fixed to the longest value ("Approval") and elapsed to a
/// wide placeholder ("9999s") to prevent jitter from state transitions or
/// ticking seconds.
fn measure_text_width(ctx: &egui::Context, text: String) -> f32 {
    let font_id = egui::FontId::proportional(11.0);
    ctx.fonts(|fonts| {
        let galley = fonts.layout_no_wrap(text, font_id, Color32::WHITE);
        galley.size().x
    })
}

fn measure_session_text_width(ctx: &egui::Context, session: &ClaudeSession) -> f32 {
    let text = format!(
        "{}  {}  [{}] {}",
        session.pane.id, session.pane.project_name, "Approval", "99h 59m"
    );
    measure_text_width(ctx, text)
}

/// Measure the rendered text width of a crmux session row.
fn measure_crmux_text_width(ctx: &egui::Context, session: &CrmuxSession) -> f32 {
    let text = format_crmux_label(session, "Approval");
    measure_text_width(ctx, text)
}

fn render_speech_bubble_with_tail(
    ui: &mut Ui,
    stroke_width: f32,
    state_color: Color32,
    bubble_fill: Color32,
    max_label_width: Option<f32>,
    content: impl FnOnce(&mut Ui),
) {
    let inner = egui::Frame::none()
        .fill(bubble_fill)
        .stroke(egui::Stroke::new(stroke_width, state_color))
        .rounding(egui::Rounding::same(5.0))
        .inner_margin(egui::Margin::symmetric(6.0, 2.0))
        .show(ui, |ui: &mut Ui| {
            if let Some(w) = max_label_width {
                ui.set_max_width(w);
            }
            content(ui);
        });

    let rect = inner.response.rect;
    let mid_y = rect.center().y;
    let tail_tip = egui::pos2(rect.left() - 4.0, mid_y);
    let tail_top = egui::pos2(rect.left(), mid_y - 4.0);
    let tail_bot = egui::pos2(rect.left(), mid_y + 4.0);
    let painter = ui.painter();
    painter.add(egui::Shape::convex_polygon(
        vec![tail_tip, tail_top, tail_bot],
        bubble_fill,
        egui::Stroke::NONE,
    ));
    painter.line_segment(
        [tail_tip, tail_top],
        egui::Stroke::new(stroke_width, state_color),
    );
    painter.line_segment(
        [tail_tip, tail_bot],
        egui::Stroke::new(stroke_width, state_color),
    );
}

fn render_robot_art(ui: &mut Ui, state_color: Color32, body_color: Color32) {
    ui.allocate_ui(egui::Vec2::new(40.0, ROW_HEIGHT), |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let lines: [(&str, Color32); 4] = [
                ("▟█▙", state_color),
                ("▐▛███▜▌", body_color),
                ("▝▜█████▛▘", body_color),
                ("▘▘ ▝▝", body_color),
            ];
            for (text, color) in lines {
                ui.label(RichText::new(text).size(5.0).color(color).monospace());
            }
        });
    });
}

fn state_color_and_label(state: &ClaudeState) -> (Color32, &'static str) {
    let color = match state {
        ClaudeState::Working => Color32::from_rgb(80, 200, 80),
        ClaudeState::WaitingForApproval => Color32::from_rgb(220, 180, 0),
        ClaudeState::Idle => Color32::from_gray(160),
    };
    (color, state.display_label())
}

/// Base fill color for speech bubbles.
fn bubble_fill_color() -> Color32 {
    Color32::from_rgba_unmultiplied(30, 30, 45, 220)
}

fn apply_opacity(color: Color32, opacity: f32) -> Color32 {
    let [r, g, b, a] = color.to_array();
    Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * opacity) as u8)
}

/// Determine whether a session should pulse based on its state, elapsed time, and quiet mode.
///
/// When `quiet` is true (Claude pane is focused and tmux client is active),
/// pulse is always suppressed.
fn should_pulse(state: &ClaudeState, elapsed_secs: u64, quiet: bool) -> bool {
    if quiet {
        return false;
    }
    matches!(state, ClaudeState::WaitingForApproval)
        || (matches!(state, ClaudeState::Idle)
            && (STALE_MIN_SECS..=STALE_MAX_SECS).contains(&elapsed_secs))
}

fn calc_stroke_width(time: f64, pulse: bool) -> f32 {
    if pulse {
        let p = ((time * 16.0).sin() as f32 + 1.0) / 2.0;
        1.0 + p * 2.0
    } else {
        1.0
    }
}

fn render_crmux_session_row(
    ui: &mut Ui,
    session: &ClaudeSession,
    crmux_session: &CrmuxSession,
    time: f64,
    hover_opacity: f32,
) {
    let (state_color, label) = state_color_and_label(&session.state);

    let state_color = apply_opacity(state_color, hover_opacity);
    let pulse = should_pulse(&session.state, crmux_session.elapsed_secs, false);
    let stroke_width = calc_stroke_width(time, pulse);

    let display_text = format_crmux_label(crmux_session, label);

    let body_color = apply_opacity(ROBOT_BODY_COLOR, hover_opacity);
    let bubble_fill = apply_opacity(bubble_fill_color(), hover_opacity);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        render_robot_art(ui, state_color, body_color);

        ui.add_space(2.0);

        let max_label_width = (ui.available_width() - 14.0).max(0.0);

        render_speech_bubble_with_tail(
            ui,
            stroke_width,
            state_color,
            bubble_fill,
            Some(max_label_width),
            |ui| {
                ui.label(
                    RichText::new(display_text)
                        .color(state_color)
                        .size(11.0),
                );
            },
        );
    });
}

fn render_session_row(
    ui: &mut Ui,
    session: &ClaudeSession,
    time: f64,
    quiet: bool,
    hover_opacity: f32,
) {
    let (state_color, label) = state_color_and_label(&session.state);

    let state_color = apply_opacity(state_color, hover_opacity);
    let elapsed = session.state_changed_at.elapsed().as_secs();
    let pulse = should_pulse(&session.state, elapsed, quiet);
    let stroke_width = calc_stroke_width(time, pulse);

    let body_color = apply_opacity(ROBOT_BODY_COLOR, hover_opacity);
    let bubble_fill = apply_opacity(bubble_fill_color(), hover_opacity);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        render_robot_art(ui, state_color, body_color);

        ui.add_space(2.0);

        let max_label_width = (ui.available_width() - 14.0).max(0.0);

        render_speech_bubble_with_tail(
            ui,
            stroke_width,
            state_color,
            bubble_fill,
            Some(max_label_width),
            |ui| {
                ui.label(
                    RichText::new(format!(
                        "{}  {}  [{}] {}",
                        session.pane.id, session.pane.project_name, label, format_elapsed(elapsed)
                    ))
                    .color(state_color)
                    .size(11.0),
                );
            },
        );
    });
}

struct StateSummary {
    state: ClaudeState,
    count: usize,
}

/// Aggregate sessions by state, returning counts in fixed display order:
/// Working -> WaitingForApproval -> Idle. States with 0 sessions are excluded.
fn state_summary(sessions: &[ClaudeSession]) -> Vec<StateSummary> {
    let display_order = [
        ClaudeState::Working,
        ClaudeState::WaitingForApproval,
        ClaudeState::Idle,
    ];
    display_order
        .into_iter()
        .filter_map(|state| {
            let count = sessions.iter().filter(|s| s.state == state).count();
            if count > 0 {
                Some(StateSummary { state, count })
            } else {
                None
            }
        })
        .collect()
}

/// Width of a single compact summary group (robot + bubble + spacing).
const COMPACT_GROUP_WIDTH: f32 = 70.0;
/// Spacing between compact summary groups.
const COMPACT_GROUP_SPACING: f32 = 4.0;

fn render_compact_row(ui: &mut Ui, summaries: &[StateSummary], time: f64, stale_idle: bool) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = COMPACT_GROUP_SPACING;
        for summary in summaries {
            let (state_color, _label) = state_color_and_label(&summary.state);

            let pulse = matches!(summary.state, ClaudeState::WaitingForApproval)
                || (stale_idle && matches!(summary.state, ClaudeState::Idle));
            let stroke_width = calc_stroke_width(time, pulse);

            let body_color = ROBOT_BODY_COLOR;
            let bubble_fill = bubble_fill_color();

            render_robot_art(ui, state_color, body_color);

            ui.add_space(2.0);

            render_speech_bubble_with_tail(
                ui,
                stroke_width,
                state_color,
                bubble_fill,
                None,
                |ui| {
                    ui.label(
                        RichText::new(summary.count.to_string())
                            .color(state_color)
                            .size(11.0),
                    );
                },
            );
        }
    });
}

fn should_alert_on_stale(sessions: &[ClaudeSession]) -> bool {
    sessions.iter().any(|s| {
        let elapsed = s.state_changed_at.elapsed().as_secs();
        match s.state {
            ClaudeState::WaitingForApproval | ClaudeState::Idle => {
                (STALE_MIN_SECS..=STALE_MAX_SECS).contains(&elapsed)
            }
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tmux_claude_state::tmux::PaneInfo;

    #[test]
    fn stroke_width_no_pulse_is_always_one() {
        assert_eq!(calc_stroke_width(0.0, false), 1.0);
        assert_eq!(calc_stroke_width(5.0, false), 1.0);
    }

    #[test]
    fn position_top_center_default() {
        let monitor = Vec2::new(1920.0, 1080.0);
        let window = Vec2::new(300.0, 40.0);
        let pos = Position::TopCenter.compute(monitor, window);
        assert_eq!(pos.x, (1920.0 - 300.0) / 2.0);
        assert_eq!(pos.y, MARGIN);
    }

    #[test]
    fn position_bottom_right() {
        let monitor = Vec2::new(1920.0, 1080.0);
        let window = Vec2::new(300.0, 40.0);
        let pos = Position::BottomRight.compute(monitor, window);
        assert_eq!(pos.x, 1920.0 - 300.0 - MARGIN);
        assert_eq!(pos.y, 1080.0 - 40.0 - MARGIN);
    }

    #[test]
    fn position_middle_center() {
        let monitor = Vec2::new(1920.0, 1080.0);
        let window = Vec2::new(300.0, 40.0);
        let pos = Position::MiddleCenter.compute(monitor, window);
        assert_eq!(pos.x, (1920.0 - 300.0) / 2.0);
        assert_eq!(pos.y, (1080.0 - 40.0) / 2.0);
    }

    #[test]
    fn stroke_width_pulse_oscillates() {
        let mut saw_peak = false;
        for t in 0..100 {
            let time = t as f64 * 0.1;
            let w = calc_stroke_width(time, true);
            assert!(w >= 1.0 && w <= 3.0, "got {w} at time {time}");
            if w > 2.5 {
                saw_peak = true;
            }
        }
        assert!(saw_peak, "should reach near 3.0");
    }

    #[test]
    fn min_window_width_is_positive_and_reasonable() {
        assert!(MIN_WINDOW_WIDTH > 0.0);
        assert!(
            MIN_WINDOW_WIDTH <= 300.0,
            "MIN_WINDOW_WIDTH should be modest"
        );
    }

    #[test]
    fn row_horizontal_overhead_is_positive() {
        assert!(ROW_HORIZONTAL_OVERHEAD > 0.0);
    }

    fn make_session(state: ClaudeState, elapsed: Duration) -> ClaudeSession {
        ClaudeSession {
            pane: PaneInfo {
                id: "test".to_string(),
                pid: 1,
                cwd: "/tmp".to_string(),
                project_name: "test-project".to_string(),
            },
            state,
            state_changed_at: Instant::now() - elapsed,
        }
    }

    #[test]
    fn state_summary_empty_sessions() {
        let result = state_summary(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn state_summary_all_working() {
        let sessions = vec![
            make_session(ClaudeState::Working, Duration::from_secs(1)),
            make_session(ClaudeState::Working, Duration::from_secs(2)),
            make_session(ClaudeState::Working, Duration::from_secs(3)),
        ];
        let result = state_summary(&sessions);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].state, ClaudeState::Working);
        assert_eq!(result[0].count, 3);
    }

    #[test]
    fn state_summary_mixed_excludes_zero() {
        let sessions = vec![
            make_session(ClaudeState::Working, Duration::from_secs(1)),
            make_session(ClaudeState::Idle, Duration::from_secs(2)),
            make_session(ClaudeState::Idle, Duration::from_secs(3)),
        ];
        let result = state_summary(&sessions);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].state, ClaudeState::Working);
        assert_eq!(result[0].count, 1);
        assert_eq!(result[1].state, ClaudeState::Idle);
        assert_eq!(result[1].count, 2);
    }

    #[test]
    fn state_summary_all_three_states() {
        let sessions = vec![
            make_session(ClaudeState::Working, Duration::from_secs(1)),
            make_session(ClaudeState::Working, Duration::from_secs(2)),
            make_session(ClaudeState::WaitingForApproval, Duration::from_secs(3)),
            make_session(ClaudeState::Idle, Duration::from_secs(4)),
            make_session(ClaudeState::Idle, Duration::from_secs(5)),
            make_session(ClaudeState::Idle, Duration::from_secs(6)),
        ];
        let result = state_summary(&sessions);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].state, ClaudeState::Working);
        assert_eq!(result[0].count, 2);
        assert_eq!(result[1].state, ClaudeState::WaitingForApproval);
        assert_eq!(result[1].count, 1);
        assert_eq!(result[2].state, ClaudeState::Idle);
        assert_eq!(result[2].count, 3);
    }

    #[test]
    fn should_alert_empty_sessions() {
        assert!(!should_alert_on_stale(&[]));
    }

    #[test]
    fn should_alert_working_never() {
        let sessions = vec![make_session(ClaudeState::Working, Duration::from_secs(10))];
        assert!(!should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_idle_under_min() {
        let sessions = vec![make_session(ClaudeState::Idle, Duration::from_secs(4))];
        assert!(!should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_idle_at_min() {
        let sessions = vec![make_session(ClaudeState::Idle, Duration::from_secs(5))];
        assert!(should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_idle_within_range() {
        let sessions = vec![make_session(ClaudeState::Idle, Duration::from_secs(10))];
        assert!(should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_idle_at_max() {
        let sessions = vec![make_session(ClaudeState::Idle, Duration::from_secs(15))];
        assert!(should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_idle_over_max() {
        let sessions = vec![make_session(ClaudeState::Idle, Duration::from_secs(16))];
        assert!(!should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_approval_under_min() {
        let sessions = vec![make_session(
            ClaudeState::WaitingForApproval,
            Duration::from_secs(3),
        )];
        assert!(!should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_approval_within_range() {
        let sessions = vec![make_session(
            ClaudeState::WaitingForApproval,
            Duration::from_secs(10),
        )];
        assert!(should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_approval_over_max() {
        let sessions = vec![make_session(
            ClaudeState::WaitingForApproval,
            Duration::from_secs(20),
        )];
        assert!(!should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_mixed_working_and_stale_idle() {
        let sessions = vec![
            make_session(ClaudeState::Working, Duration::from_secs(30)),
            make_session(ClaudeState::Idle, Duration::from_secs(10)),
        ];
        assert!(should_alert_on_stale(&sessions));
    }

    #[test]
    fn should_alert_mixed_working_and_expired_idle() {
        let sessions = vec![
            make_session(ClaudeState::Working, Duration::from_secs(30)),
            make_session(ClaudeState::Idle, Duration::from_secs(20)),
        ];
        assert!(!should_alert_on_stale(&sessions));
    }

    // --- should_pulse ---

    #[test]
    fn pulse_approval_not_quiet() {
        assert!(should_pulse(&ClaudeState::WaitingForApproval, 0, false));
    }

    #[test]
    fn pulse_approval_quiet_suppressed() {
        assert!(!should_pulse(&ClaudeState::WaitingForApproval, 0, true));
    }

    #[test]
    fn pulse_idle_stale_not_quiet() {
        assert!(should_pulse(&ClaudeState::Idle, 10, false));
    }

    #[test]
    fn pulse_idle_stale_quiet_suppressed() {
        assert!(!should_pulse(&ClaudeState::Idle, 10, true));
    }

    #[test]
    fn pulse_idle_fresh_not_quiet() {
        assert!(!should_pulse(&ClaudeState::Idle, 3, false));
    }

    #[test]
    fn pulse_idle_expired_not_quiet() {
        assert!(!should_pulse(&ClaudeState::Idle, 20, false));
    }

    #[test]
    fn pulse_working_not_quiet() {
        assert!(!should_pulse(&ClaudeState::Working, 10, false));
    }

    #[test]
    fn pulse_working_quiet() {
        assert!(!should_pulse(&ClaudeState::Working, 10, true));
    }

    #[test]
    fn apply_opacity_full() {
        let c = Color32::from_rgba_unmultiplied(100, 150, 200, 255);
        let result = apply_opacity(c, 1.0);
        assert_eq!(result, Color32::from_rgba_unmultiplied(100, 150, 200, 255));
    }

    #[test]
    fn apply_opacity_half_alpha() {
        let c = Color32::from_rgba_unmultiplied(100, 150, 200, 200);
        let result = apply_opacity(c, 0.5);
        assert_eq!(result.a(), 100);
    }

    #[test]
    fn apply_opacity_zero() {
        let c = Color32::from_rgb(100, 150, 200);
        let result = apply_opacity(c, 0.0);
        assert_eq!(result, Color32::from_rgba_unmultiplied(100, 150, 200, 0));
    }

    // --- format_elapsed ---

    #[test]
    fn format_elapsed_seconds() {
        assert_eq!(format_elapsed(0), "0s");
        assert_eq!(format_elapsed(45), "45s");
        assert_eq!(format_elapsed(59), "59s");
    }

    #[test]
    fn format_elapsed_minutes() {
        assert_eq!(format_elapsed(60), "1m");
        assert_eq!(format_elapsed(90), "1m 30s");
        assert_eq!(format_elapsed(3599), "59m 59s");
    }

    #[test]
    fn format_elapsed_hours() {
        assert_eq!(format_elapsed(3600), "1h");
        assert_eq!(format_elapsed(3660), "1h 1m");
        assert_eq!(format_elapsed(7200), "2h");
        assert_eq!(format_elapsed(7320), "2h 2m");
    }

    // --- parse_crmux_state ---

    #[test]
    fn parse_crmux_state_working() {
        assert_eq!(parse_crmux_state("Working"), ClaudeState::Working);
    }

    #[test]
    fn parse_crmux_state_waiting_for_approval() {
        assert_eq!(
            parse_crmux_state("WaitingForApproval"),
            ClaudeState::WaitingForApproval
        );
    }

    #[test]
    fn parse_crmux_state_approval_alias() {
        assert_eq!(
            parse_crmux_state("Approval"),
            ClaudeState::WaitingForApproval
        );
    }

    #[test]
    fn parse_crmux_state_idle() {
        assert_eq!(parse_crmux_state("Idle"), ClaudeState::Idle);
    }

    #[test]
    fn parse_crmux_state_unknown_defaults_to_idle() {
        assert_eq!(parse_crmux_state("Unknown"), ClaudeState::Idle);
    }

    // --- crmux_to_claude_sessions ---

    #[test]
    fn crmux_to_claude_sessions_empty() {
        let result = crmux_to_claude_sessions(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn crmux_to_claude_sessions_converts_fields() {
        let crmux = CrmuxSession {
            pane_id: "%5".to_string(),
            pid: 123,
            project_name: "myproject".to_string(),
            state: "Working".to_string(),
            elapsed_secs: 30,
            model: Some("Opus".to_string()),
            context_percent: Some(50),
            title: Some("doing stuff".to_string()),
            session_id: Some("sess-1".to_string()),
            git_branch: Some("main".to_string()),
        };
        let result = crmux_to_claude_sessions(&[crmux]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].pane.id, "%5");
        assert_eq!(result[0].pane.pid, 123);
        assert_eq!(result[0].pane.project_name, "myproject");
        assert_eq!(result[0].state, ClaudeState::Working);
        // elapsed should be approximately 30 seconds
        let elapsed = result[0].state_changed_at.elapsed().as_secs();
        assert!(elapsed >= 29 && elapsed <= 32);
    }

    // --- format_crmux_label ---

    fn make_crmux_session(
        project: &str,
        branch: Option<&str>,
        model: Option<&str>,
        context: Option<u32>,
        elapsed: u64,
    ) -> CrmuxSession {
        make_crmux_session_with_title(project, branch, None, model, context, elapsed)
    }

    fn make_crmux_session_with_title(
        project: &str,
        branch: Option<&str>,
        title: Option<&str>,
        model: Option<&str>,
        context: Option<u32>,
        elapsed: u64,
    ) -> CrmuxSession {
        CrmuxSession {
            pane_id: "%1".to_string(),
            pid: 1,
            project_name: project.to_string(),
            state: "Working".to_string(),
            elapsed_secs: elapsed,
            model: model.map(|s| s.to_string()),
            context_percent: context,
            title: title.map(|s| s.to_string()),
            session_id: None,
            git_branch: branch.map(|s| s.to_string()),
        }
    }

    #[test]
    fn format_crmux_label_full_info() {
        let s = make_crmux_session("crmux", Some("main"), Some("Opus"), Some(23), 45);
        let label = format_crmux_label(&s, "Running");
        assert_eq!(label, "crmux (main)  Opus  23%  [Running] 45s");
    }

    #[test]
    fn format_crmux_label_no_branch() {
        let s = make_crmux_session("myproj", None, Some("Sonnet"), Some(10), 5);
        let label = format_crmux_label(&s, "Idle");
        assert_eq!(label, "myproj  Sonnet  10%  [Idle] 5s");
    }

    #[test]
    fn format_crmux_label_no_model_no_context() {
        let s = make_crmux_session("proj", Some("dev"), None, None, 100);
        let label = format_crmux_label(&s, "Approval");
        assert_eq!(label, "proj (dev)  [Approval] 1m 40s");
    }

    #[test]
    fn format_crmux_label_minimal() {
        let s = make_crmux_session("proj", None, None, None, 0);
        let label = format_crmux_label(&s, "Running");
        assert_eq!(label, "proj  [Running] 0s");
    }

    #[test]
    fn format_crmux_label_with_title() {
        let s = make_crmux_session_with_title(
            "crmux",
            Some("main"),
            Some("implementing feature X"),
            Some("Opus"),
            Some(23),
            45,
        );
        let label = format_crmux_label(&s, "Running");
        assert_eq!(
            label,
            "crmux (main)  implementing feature…  Opus  23%  [Running] 45s"
        );
    }

    #[test]
    fn format_crmux_label_with_title_no_branch() {
        let s = make_crmux_session_with_title("proj", None, Some("fix bug"), None, None, 10);
        let label = format_crmux_label(&s, "Idle");
        assert_eq!(label, "proj  fix bug  [Idle] 10s");
    }

    #[test]
    fn format_crmux_label_title_newlines_replaced() {
        let s = make_crmux_session_with_title("proj", None, Some("line1\nline2\nline3"), None, None, 5);
        let label = format_crmux_label(&s, "Running");
        assert_eq!(label, "proj  line1 line2 line3  [Running] 5s");
    }

    #[test]
    fn format_crmux_label_title_truncated_at_40_chars() {
        let long_title = "abcdefghijklmnopqrstuvwxyz";
        assert!(long_title.chars().count() > 20);
        let s = make_crmux_session_with_title("proj", None, Some(long_title), None, None, 5);
        let label = format_crmux_label(&s, "Running");
        // first 20 chars + ellipsis
        assert!(label.contains("abcdefghijklmnopqrst…"));
        assert!(!label.contains("uvwxyz"));
    }

    #[test]
    fn format_crmux_label_empty_title_skipped() {
        let s = make_crmux_session_with_title("proj", None, Some(""), None, None, 5);
        let label = format_crmux_label(&s, "Running");
        assert_eq!(label, "proj  [Running] 5s");
    }

    #[test]
    fn format_crmux_label_with_japanese_title() {
        let s = make_crmux_session_with_title(
            "crmux",
            Some("main"),
            Some("日本語タイトルのテスト"),
            Some("Opus"),
            Some(42),
            60,
        );
        let label = format_crmux_label(&s, "Running");
        assert_eq!(
            label,
            "crmux (main)  日本語タイトルのテスト  Opus  42%  [Running] 1m"
        );
    }
}
