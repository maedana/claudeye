mod claude_state;
mod monitor;
mod picker;
mod tmux;

use clap::{Parser, Subcommand};
use eframe::egui::{self, Color32, RichText, Ui, Vec2};
use monitor::{ClaudeSession, start_polling};
use claude_state::ClaudeState;
use std::sync::{Arc, Mutex};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    match args.command {
        Some(Commands::Picker) => picker::run_picker()?,
        None => run_gui(args.compact, args.position, args.alert_on_stale)?,
    }
    Ok(())
}

fn run_gui(compact: bool, position: Position, alert_on_stale: bool) -> eframe::Result<()> {
    let sessions: Arc<Mutex<Vec<ClaudeSession>>> = Arc::new(Mutex::new(vec![]));
    start_polling(Arc::clone(&sessions));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_decorations(false)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_inner_size([MIN_WINDOW_WIDTH, WINDOW_EMPTY_HEIGHT])
            .with_transparent(true),
        ..Default::default()
    };

    eframe::run_native(
        "claudeye",
        options,
        Box::new(|_cc| Ok(Box::new(CcMonitorApp { sessions, compact, position, alert_on_stale }))),
    )
}

struct CcMonitorApp {
    sessions: Arc<Mutex<Vec<ClaudeSession>>>,
    compact: bool,
    position: Position,
    alert_on_stale: bool,
}

impl eframe::App for CcMonitorApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let mut visuals = ctx.style().visuals.clone();
        visuals.panel_fill = Color32::TRANSPARENT;
        ctx.set_visuals(visuals);

        ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(egui::WindowLevel::AlwaysOnTop));
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(true));

        let sessions = match self.sessions.lock() {
            Ok(guard) => guard.clone(),
            Err(_) => return, // poisoned mutex: polling thread panicked
        };

        let has_approval = sessions.iter().any(|s| matches!(s.state, ClaudeState::WaitingForApproval));
        if self.compact {
            if has_approval {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            } else if !sessions.is_empty() {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs(REPAINT_INTERVAL_SECS));
            }
        } else {
            let needs_fast_repaint = sessions.iter().any(|s| matches!(s.state, ClaudeState::Working | ClaudeState::WaitingForApproval));
            if needs_fast_repaint {
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
            } else if !sessions.is_empty() {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs(REPAINT_INTERVAL_SECS));
            }
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
                let effective_position = if self.alert_on_stale && should_alert_on_stale(&sessions) {
                    Position::MiddleCenter
                } else {
                    self.position
                };
                let pos = effective_position.compute(monitor_size, Vec2::new(window_width, window_height));
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
                        render_compact_row(ui, &summaries, time);
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

            if let Some(monitor_size) = ctx.input(|i| i.viewport().monitor_size) {
                let effective_position = if self.alert_on_stale && should_alert_on_stale(&sessions) {
                    Position::MiddleCenter
                } else {
                    self.position
                };
                let pos = effective_position.compute(monitor_size, Vec2::new(window_width, window_height));
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
            }

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
                                .color(Color32::from_gray(120))
                                .size(12.0),
                        );
                    } else {
                        for session in &display_sessions {
                            render_session_row(ui, session, time);
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
fn measure_session_text_width(ctx: &egui::Context, session: &ClaudeSession) -> f32 {
    let text = format!(
        "{}  {}  [{}] {}",
        session.pane.id, session.pane.project_name, "Approval", "9999s"
    );
    let font_id = egui::FontId::proportional(11.0);
    ctx.fonts(|fonts| {
        let galley = fonts.layout_no_wrap(text, font_id, Color32::WHITE);
        galley.size().x
    })
}

fn calc_stroke_width(state: &ClaudeState, time: f64) -> f32 {
    match state {
        ClaudeState::WaitingForApproval => {
            let pulse = ((time * 16.0).sin() as f32 + 1.0) / 2.0;
            1.0 + pulse * 2.0
        }
        ClaudeState::Working | ClaudeState::Idle => 1.0,
    }
}

fn render_session_row(ui: &mut Ui, session: &ClaudeSession, time: f64) {
    let (state_color, label) = match &session.state {
        ClaudeState::Working => (Color32::from_rgb(80, 200, 80), "Running"),
        ClaudeState::WaitingForApproval => (Color32::from_rgb(220, 180, 0), "Approval"),
        ClaudeState::Idle => (Color32::from_gray(160), "Idle"),
    };

    let stroke_width = calc_stroke_width(&session.state, time);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        // Mini robot art or spinner (fixed-width column, center-aligned)
        ui.allocate_ui(egui::Vec2::new(40.0, ROW_HEIGHT), |ui| {
            ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let o = Color32::from_rgb(210, 110, 30);  // orange
                let lines: [(&str, Color32); 4] = [
                    ("▟█▙", state_color),
                    ("▐▛███▜▌", o),
                    ("▝▜█████▛▘", o),
                    ("▘▘ ▝▝", o),
                ];
                for (text, color) in lines {
                    ui.label(RichText::new(text).size(5.0).color(color).monospace());
                }
            });
        });

        // Speech bubble with tail pointing left toward robot
        ui.add_space(2.0); // space for the tail triangle

        // Clamp bubble width to remaining available space (minus inner padding + stroke)
        let max_label_width = (ui.available_width() - 14.0).max(0.0);

        let bubble_fill = Color32::from_rgba_unmultiplied(30, 30, 45, 220);
        let inner = egui::Frame::none()
            .fill(bubble_fill)
            .stroke(egui::Stroke::new(stroke_width, state_color))
            .rounding(egui::Rounding::same(5.0))
            .inner_margin(egui::Margin::symmetric(6.0, 2.0))
            .show(ui, |ui: &mut Ui| {
                ui.set_max_width(max_label_width);
                let elapsed = session.state_changed_at.elapsed().as_secs();
                ui.label(
                    RichText::new(format!(
                        "{}  {}  [{}] {}s",
                        session.pane.id, session.pane.project_name, label, elapsed
                    ))
                    .color(state_color)
                    .size(11.0),
                );
            });

        // Draw tail triangle pointing left toward the robot
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
        painter.line_segment([tail_tip, tail_top], egui::Stroke::new(stroke_width, state_color));
        painter.line_segment([tail_tip, tail_bot], egui::Stroke::new(stroke_width, state_color));
    });
}

struct StateSummary {
    state: ClaudeState,
    count: usize,
}

/// Aggregate sessions by state, returning counts in fixed display order:
/// Working → WaitingForApproval → Idle. States with 0 sessions are excluded.
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

fn render_compact_row(ui: &mut Ui, summaries: &[StateSummary], time: f64) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = COMPACT_GROUP_SPACING;
        for summary in summaries {
            let (state_color, _label) = match &summary.state {
                ClaudeState::Working => (Color32::from_rgb(80, 200, 80), "Running"),
                ClaudeState::WaitingForApproval => (Color32::from_rgb(220, 180, 0), "Approval"),
                ClaudeState::Idle => (Color32::from_gray(160), "Idle"),
            };

            let stroke_width = calc_stroke_width(&summary.state, time);

            // Mini robot art (same as render_session_row)
            ui.allocate_ui(egui::Vec2::new(40.0, ROW_HEIGHT), |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    let o = Color32::from_rgb(210, 110, 30);
                    let lines: [(&str, Color32); 4] = [
                        ("▟█▙", state_color),
                        ("▐▛███▜▌", o),
                        ("▝▜█████▛▘", o),
                        ("▘▘ ▝▝", o),
                    ];
                    for (text, color) in lines {
                        ui.label(RichText::new(text).size(5.0).color(color).monospace());
                    }
                });
            });

            // Speech bubble with count
            ui.add_space(2.0);
            let bubble_fill = Color32::from_rgba_unmultiplied(30, 30, 45, 220);
            let inner = egui::Frame::none()
                .fill(bubble_fill)
                .stroke(egui::Stroke::new(stroke_width, state_color))
                .rounding(egui::Rounding::same(5.0))
                .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                .show(ui, |ui: &mut Ui| {
                    ui.label(
                        RichText::new(format!("{}", summary.count))
                            .color(state_color)
                            .size(11.0),
                    );
                });

            // Draw tail triangle pointing left toward the robot
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
            painter.line_segment([tail_tip, tail_top], egui::Stroke::new(stroke_width, state_color));
            painter.line_segment([tail_tip, tail_bot], egui::Stroke::new(stroke_width, state_color));
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
    use crate::tmux::PaneInfo;

    #[test]
    fn stroke_width_working_is_always_one() {
        assert_eq!(calc_stroke_width(&ClaudeState::Working, 0.0), 1.0);
        assert_eq!(calc_stroke_width(&ClaudeState::Working, 5.0), 1.0);
    }

    #[test]
    fn stroke_width_idle_is_always_one() {
        assert_eq!(calc_stroke_width(&ClaudeState::Idle, 0.0), 1.0);
        assert_eq!(calc_stroke_width(&ClaudeState::Idle, 5.0), 1.0);
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
    fn stroke_width_approval_always_pulses_strongly() {
        let mut saw_peak = false;
        for t in 0..100 {
            let time = t as f64 * 0.1;
            let w = calc_stroke_width(&ClaudeState::WaitingForApproval, time);
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
        assert!(MIN_WINDOW_WIDTH <= 300.0, "MIN_WINDOW_WIDTH should be modest");
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
}
