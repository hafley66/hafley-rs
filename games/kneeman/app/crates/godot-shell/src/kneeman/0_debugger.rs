//! Local tick-boundary controls, bounded terrain projections, and serialized-snapshot replay.
//! Projection scrubbing never writes simulation state. Verification runs a separate restored copy.

use super::{KneeMan, Phase, gv};
use crate::sim::{self, InputFrame, SimState, Tune};
use godot::prelude::*;
use std::collections::VecDeque;

const HISTORY: usize = 240;
const CAPTURE: usize = 600;

#[derive(Clone, Copy)]
enum Command {
    Pause,
    Resume,
    Step,
    Capture,
    Verify,
    Restore,
    Fixture,
}

struct Trace {
    snapshot: Vec<u8>,
    tune: Tune,
    tune_bytes: Vec<u8>,
    expected: u128,
    frames: Vec<([InputFrame; 2], u128)>,
    recording: bool,
}

impl Trace {
    fn new(state: &SimState, tune: &Tune) -> Self {
        Self {
            snapshot: bincode::serialize(state).unwrap(),
            tune: tune.clone(),
            tune_bytes: bincode::serialize(tune).unwrap(),
            expected: sim::net::checksum(state),
            frames: Vec::new(),
            recording: true,
        }
    }

    fn verify(&self) -> Result<usize, String> {
        let mut state: SimState =
            bincode::deserialize(&self.snapshot).map_err(|e| e.to_string())?;
        for (index, (inputs, expected)) in self.frames.iter().enumerate() {
            state = sim::step(&state, &[&inputs[0], &inputs[1]], &self.tune);
            let actual = sim::net::checksum(&state);
            if actual != *expected {
                return Err(format!(
                    "Mismatch at capture frame {index}, tick {}: {actual:x} != {expected:x}",
                    state.tick
                ));
            }
        }
        Ok(self.frames.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_capture_verifies_every_tick_and_reports_first_corruption() {
        let tune = Tune::default();
        let mut state = SimState::spawn();
        let mut debugger = Debugger {
            trace: Some(Trace::new(&state, &tune)),
            ..Debugger::default()
        };
        for _ in 0..60 {
            let inputs = [InputFrame::default(); 2];
            let next = sim::step(&state, &[&inputs[0], &inputs[1]], &tune);
            debugger.record(&state, &next, inputs, &tune);
            state = next;
        }
        let trace = debugger.trace.as_mut().unwrap();
        assert_eq!(trace.verify(), Ok(60));
        trace.frames[17].1 ^= 1;
        assert!(
            trace
                .verify()
                .unwrap_err()
                .starts_with("Mismatch at capture frame 17, tick 18:")
        );
    }

    #[test]
    fn external_edits_stop_capture_and_projection_history_is_bounded() {
        let tune = Tune::default();
        let mut state = SimState::spawn();
        let mut debugger = Debugger {
            trace: Some(Trace::new(&state, &tune)),
            ..Debugger::default()
        };
        state.fighters[0].damage = 42.0;
        debugger.record(&state, &state, [InputFrame::default(); 2], &tune);
        let trace = debugger.trace.as_ref().unwrap();
        assert!(!trace.recording);
        assert_eq!(trace.frames.len(), 0);
        assert_eq!(trace.verify(), Ok(0));
        for tick in 0..300 {
            state.tick = tick;
            debugger.observe(&state);
        }
        assert_eq!(debugger.history.len(), HISTORY);
        assert_eq!(debugger.history.front().unwrap().tick, 60);
        assert_eq!(debugger.history.back().unwrap().tick, 299);
        state.tick = 1;
        debugger.observe(&state);
        assert_eq!(debugger.history.len(), 1);
        assert_eq!(debugger.history[0].tick, 1);
    }
}

struct Row {
    id: u64,
    status: &'static str,
    hp: f32,
    pos: sim::Vector2,
    vel: sim::Vector2,
    broken: Option<u64>,
    points: Vec<sim::Vector2>,
}
struct Frame {
    tick: u64,
    rows: Vec<Row>,
    fighters: Vec<sim::Vector2>,
}

#[derive(Default)]
pub(super) struct Debugger {
    pub paused: bool,
    pub isolated: bool,
    command: Option<Command>,
    trace: Option<Trace>,
    history: VecDeque<Frame>,
    selected: Option<usize>,
    message: String,
}

impl Debugger {
    pub fn record(
        &mut self,
        before: &SimState,
        after: &SimState,
        inputs: [InputFrame; 2],
        tune: &Tune,
    ) {
        let Some(trace) = &mut self.trace else {
            return;
        };
        if !trace.recording {
            return;
        }
        if sim::net::checksum(before) != trace.expected
            || bincode::serialize(tune).unwrap() != trace.tune_bytes
        {
            trace.recording = false;
            self.message = "Capture stopped: an external state edit or tuning change interrupted the input-only sequence.".into();
            return;
        }
        trace.expected = sim::net::checksum(after);
        trace.frames.push((inputs, trace.expected));
        if trace.frames.len() == CAPTURE {
            trace.recording = false;
            self.message = format!("Captured {CAPTURE} ticks; ready to verify.");
        }
    }

    pub fn observe(&mut self, state: &SimState) {
        if self
            .history
            .back()
            .is_some_and(|frame| frame.tick == state.tick)
        {
            return;
        }
        if self
            .history
            .back()
            .is_some_and(|frame| frame.tick > state.tick)
        {
            self.history.clear();
            self.selected = None;
        }
        let mut rows = Vec::new();
        for path in &state.paths {
            let Some(cell) = path.cell.filter(|_| path.active()) else {
                continue;
            };
            rows.push(Row {
                id: cell.id.get(),
                status: if path.traveling() {
                    "moving terrain"
                } else {
                    "attached"
                },
                hp: cell.durability - path.percent,
                pos: path.pos,
                vel: path.vel / sim::DT,
                broken: None,
                points: (0..path.len as usize)
                    .map(|i| path.world_pt(i, &state.nodes))
                    .collect(),
            });
        }
        for item in &state.items {
            let Some(cell) = item.cell.filter(|_| item.active()) else {
                continue;
            };
            rows.push(Row {
                id: cell.id.get(),
                status: if item.thrown {
                    "thrown"
                } else if item.owner >= 0 {
                    "held"
                } else {
                    "loose"
                },
                hp: item.hp,
                pos: item.pos,
                vel: item.vel,
                broken: cell.broken_at.map(|t| t.get()),
                points: Vec::new(),
            });
        }
        rows.sort_by_key(|row| row.id);
        self.history.push_back(Frame {
            tick: state.tick,
            rows,
            fighters: state.fighters[..state.active as usize]
                .iter()
                .map(|f| f.pos)
                .collect(),
        });
        if self.history.len() > HISTORY {
            self.history.pop_front();
            self.selected = self.selected.map(|i| i.saturating_sub(1));
        }
    }

    fn view(&mut self, ctx: &egui::Context, offline: bool) {
        egui::Window::new("Terrain & replay").default_width(540.0).show(ctx, |ui| {
            ui.add_enabled_ui(offline, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (label, command) in [("Pause", Command::Pause), ("Resume", Command::Resume),
                        ("Step", Command::Step), ("Capture", Command::Capture), ("Verify replay", Command::Verify),
                        ("Restore start", Command::Restore), ("Load fixture", Command::Fixture)] {
                        if ui.button(label).clicked() { self.command = Some(command); }
                    }
                });
            });
            if !offline { ui.label("Controls are local-only. Network simulation continues normally."); }
            if self.isolated { ui.label("Debug state: automatic world saving disabled until scene reload."); }
            if let Some(trace) = &self.trace { ui.label(format!("{} recorded ticks; {}", trace.frames.len(),
                if trace.recording { "recording" } else { "stopped" })); }
            ui.label(&self.message);
            ui.label("Scrub = recorded projections. Verify replay = deserialize + re-simulate + compare every tick.");
            if self.history.is_empty() { return; }
            let last = self.history.len() - 1;
            let mut selected = self.selected.unwrap_or(last).min(last);
            ui.horizontal(|ui| {
                if ui.add(egui::Slider::new(&mut selected, 0..=last).text("history")).changed() { self.selected = Some(selected); }
                if ui.button("Live").clicked() { self.selected = None; selected = last; }
            });
            let frame = &self.history[selected];
            ui.label(format!("Tick {} | {} cells", frame.tick, frame.rows.len()));
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 160.0), egui::Sense::hover());
            let painter = ui.painter_at(rect);
            painter.rect_filled(rect, 0.0, egui::Color32::from_gray(20));
            let project = |p: sim::Vector2| egui::pos2(rect.left() + (p.x + 100.0) * rect.width() / 1400.0,
                rect.top() + (p.y - 200.0) * rect.height() / 800.0);
            for row in &frame.rows {
                let color = if row.broken.is_some() { egui::Color32::GOLD } else { egui::Color32::LIGHT_BLUE };
                if row.points.is_empty() { painter.circle_filled(project(row.pos), 4.0, color); }
                for pair in row.points.windows(2) { painter.line_segment([project(pair[0]), project(pair[1])], (1.5, color)); }
            }
            for pos in &frame.fighters { painter.circle_filled(project(*pos), 3.0, egui::Color32::LIGHT_GREEN); }
            egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                egui::Grid::new("terrain_rows").striped(true).show(ui, |ui| {
                    for label in ["id", "state", "HP", "velocity px/s", "break tick"] { ui.label(label); } ui.end_row();
                    for row in &frame.rows {
                        ui.label(row.id.to_string()); ui.label(row.status); ui.label(format!("{:.1}", row.hp));
                        ui.label(format!("{:.1}, {:.1}", row.vel.x, row.vel.y));
                        ui.label(row.broken.map(|t| t.to_string()).unwrap_or_default()); ui.end_row();
                    }
                });
            });
        });
    }
}

impl KneeMan {
    pub(crate) fn draw_debugger(&mut self, ctx: &egui::Context) {
        self.debugger.view(ctx, self.phase == Phase::Offline);
    }

    /// Apply the pending UI command exactly once at the start of the next physics tick.
    pub(super) fn debug_tick(&mut self) -> bool {
        let command = self.debugger.command.take();
        if self.phase != Phase::Offline {
            return false;
        }
        match command {
            Some(Command::Pause) => self.debugger.paused = true,
            Some(Command::Resume) => self.debugger.paused = false,
            Some(Command::Step) => {
                self.debugger.paused = true;
                return true;
            }
            Some(Command::Capture) => {
                self.debugger.trace = Some(Trace::new(&self.state.get(), &self.tune.get_cloned()));
                self.debugger.message =
                    "Recording up to 600 input-only ticks. Resume to advance.".into();
            }
            Some(Command::Verify) => {
                if let Some(trace) = &mut self.debugger.trace {
                    trace.recording = false;
                    self.debugger.message = match trace.verify() {
                        Ok(0) => "No recorded ticks to verify.".into(),
                        Ok(n) => {
                            format!("PASS: restored snapshot and matched all {n} tick checksums.")
                        }
                        Err(error) => error,
                    };
                }
            }
            Some(Command::Restore) => {
                if let Some(trace) = &mut self.debugger.trace {
                    trace.recording = false;
                    if let Ok(state) = bincode::deserialize::<SimState>(&trace.snapshot) {
                        self.state.set(state);
                        self.tune.set(trace.tune.clone());
                        self.debugger.paused = true;
                        self.debugger.isolated = true;
                        self.debugger.history.clear();
                        self.debugger.selected = None;
                    }
                }
            }
            Some(Command::Fixture) => {
                let mut state = SimState::spawn();
                state.fighters[0].pos = sim::Vector2::new(420.0, 760.0);
                state.fighters[0].state = sim::CharState::Stand;
                state.fighters[0].ground_plat = 0;
                state.tick = 1;
                sim::terrain_cells::spawn_cells(
                    &mut state,
                    0,
                    sim::Vector2::new(550.0, 650.0),
                    sim::Vector2::ZERO,
                    sim::StrokeProps::TETRIS,
                    0,
                    10.0,
                );
                state.items[1] = sim::Item {
                    kind: sim::ItemKind::TetrisDropper,
                    pos: sim::Vector2::new(440.0, 700.0),
                    gas: 8.0,
                    gas_max: 8.0,
                    stroke: sim::StrokeRegistry::TETRIS_ROW,
                    hp: 20.0,
                    ..sim::Item::EMPTY
                };
                self.state.set(state);
                self.tune.set(Tune::default());
                self.debugger = Debugger { paused: true, isolated: true,
                    message: "Fixture paused. Resume; jump and strike the blue cells. The dropper is beside P1.".into(),
                    ..Debugger::default() };
            }
            None => {}
        }
        if command.is_some() {
            let state = self.state.get();
            self.base_mut().set_position(gv(state.fighters[0].pos));
            self.render_fighters(&state);
            self.base_mut().queue_redraw();
        }
        false
    }
}
