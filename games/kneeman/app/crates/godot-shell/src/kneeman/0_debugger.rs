//! Local tick-boundary controls, bounded terrain projections, and serialized-snapshot replay.
//! Projection scrubbing never writes simulation state. Verification runs a separate restored copy.

use super::{KneeMan, Phase, gv};
use crate::sim::{self, InputFrame, SimState, Tune};
use godot::prelude::*;
use std::collections::VecDeque;

const HISTORY: usize = 240;
const CAPTURE: usize = 600;

#[derive(Clone, Copy)]
enum DiveStart { Catch, Whiff, Ship }

#[derive(Clone, Copy)]
enum Command {
    Pause,
    Resume,
    Step,
    Capture,
    Verify,
    Restore,
    Fixture,
    CellReplay,
    JumpGrab,
    Dive(DiveStart),
    KickContact(bool),
    KickTravel(bool),
    FairContact(bool),
    ReplayStep,
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
    fn idle(mut state: SimState) -> Self {
        let tune = Tune::default();
        let mut trace = Self::new(&state, &tune);
        trace.recording = false;
        for _ in 0..60 {
            let inputs = [InputFrame::default(); 2];
            state = sim::step(&state, &[&inputs[0], &inputs[1]], &tune);
            trace.frames.push((inputs, sim::net::checksum(&state)));
        }
        trace
    }

    fn cells() -> Self {
        let tune = Tune::default();
        let mut state = sim::terrain_cells::playground();
        let mut trace = Self::new(&state, &tune);
        trace.recording = false;
        for input in sim::terrain_cells::playground_inputs() {
            let inputs = [input, InputFrame::default()];
            state = sim::step(&state, &[&inputs[0], &inputs[1]], &tune);
            trace.frames.push((inputs, sim::net::checksum(&state)));
        }
        trace
    }

    fn falcon_dive(start: DiveStart) -> Self {
        let tune = Tune::default();
        let mut state = SimState::spawn();
        state.fighters[0].char_id = 2;
        state.fighters[1].char_id = 3;
        for _ in 0..60 { state = sim::step(&state, &[&InputFrame::default(); 2], &tune); }
        match start {
            DiveStart::Catch => state.fighters[1].pos.x = state.fighters[0].pos.x + 90.0,
            DiveStart::Whiff => state.fighters[1].pos.x = state.fighters[0].pos.x + 300.0,
            DiveStart::Ship => {
                state.paths[sim::SHIP_SLOT].pos = sim::Vector2::new(600.0, 100.0);
                state.paths[sim::SHIP_SLOT].vel = sim::Vector2::new(1.0, -1.0);
                let fighter = &mut state.fighters[0];
                fighter.pos = sim::Vector2::new(600.0, 100.0 + sim::SHIP_R);
                fighter.vel = sim::Vector2::ZERO;
                fighter.ground_plat = 0;
                fighter.ground_ink = sim::SHIP_SLOT as i8;
                state.fighters[1].pos = sim::Vector2::new(900.0, sim::PLATFORMS[0].y);
                state.fighters[1].ground_plat = 0;
            }
        }
        let mut trace = Self::new(&state, &tune);
        trace.recording = false;
        for tick in 0..180 {
            let input = InputFrame { special: tick == 0, aim_y: if tick == 0 { -1.0 } else { 0.0 },
                ..InputFrame::default() };
            let inputs = [sim::net::decode(sim::net::encode(&input)), InputFrame::default()];
            state = sim::step(&state, &[&inputs[0], &inputs[1]], &tune);
            trace.frames.push((inputs, sim::net::checksum(&state)));
        }
        trace
    }

    fn jump_grab() -> Self {
        let tune = Tune::default();
        let mut state = SimState::spawn();
        state.fighters[0].char_id = 2;
        state.fighters[1].char_id = 3;
        for _ in 0..60 { state = sim::step(&state, &[&InputFrame::default(); 2], &tune); }
        let mut trace = Self::new(&state, &tune);
        trace.recording = false;
        for tick in 0..45 {
            let mut input = InputFrame::default();
            input.jump = tick == 0;
            input.jump_held = tick < 2;
            input.grab = tick == 1;
            let inputs = [sim::net::decode(sim::net::encode(&input)), InputFrame::default()];
            state = sim::step(&state, &[&inputs[0], &inputs[1]], &tune);
            trace.frames.push((inputs, sim::net::checksum(&state)));
        }
        trace
    }

    fn replay_step(&self, state: &SimState, index: usize) -> Result<Option<SimState>, String> {
        let Some((inputs, expected)) = self.frames.get(index) else { return Ok(None); };
        let before = if index == 0 {
            let initial: SimState = bincode::deserialize(&self.snapshot).map_err(|e| e.to_string())?;
            sim::net::checksum(&initial)
        } else { self.frames[index - 1].1 };
        if sim::net::checksum(state) != before { return Err("Replay state changed; restore start first.".into()); }
        let next = sim::step(state, &[&inputs[0], &inputs[1]], &self.tune);
        if sim::net::checksum(&next) != *expected { return Err(format!("Replay mismatch at frame {index}.")); }
        Ok(Some(next))
    }

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
    #[test]
    fn kick_travel_replays_both_entry_rows_and_preserves_captured_tuning() {
        for air in [false, true] {
            let trace = super::Trace::idle(super::sim::fixtures::kick_travel(air));
            assert_eq!(trace.verify(), Ok(60));
            let start: super::SimState = bincode::deserialize(&trace.snapshot).unwrap();
            let mut state = start;
            for index in 0..60 {
                state = trace.replay_step(&state, index).unwrap().unwrap();
                assert_eq!(state.fighters[1].damage, if air { 11.0 } else { 9.0 });
            }
            assert!(trace.replay_step(&state, 60).unwrap().is_none());
            assert!(trace.replay_step(&state, 0).is_err());

            let mut edited = trace.tune.clone();
            let row = super::sim::art_slot_row(2);
            let special = &mut std::sync::Arc::make_mut(&mut edited.roster)[row].specials[3];
            let ground = special.hit;
            let landing = special.landing;
            for hit in &mut special.air_hit.as_mut().unwrap().boxes { hit.damage = 20.5; }
            assert!(special.hit == ground, "air edit changed ground attack");
            assert!(special.landing == landing, "air edit changed landing attack");
            let stepped = super::sim::step(&start, &[&super::InputFrame::default(); 2], &edited);
            assert_eq!(stepped.fighters[1].damage, if air { 20.5 } else { 9.0 });
            let captured = trace.replay_step(&start, 0).unwrap().unwrap();
            assert_eq!(captured.fighters[1].damage, if air { 11.0 } else { 9.0 });
        }
    }

    #[test]
    fn kick_contact_replays_ground_hit_and_air_rejection() {
        for grounded in [false, true] {
            let trace = super::Trace::idle(super::sim::fixtures::kick_contact(grounded));
            assert_eq!(trace.verify(), Ok(60));
            let mut state: super::SimState = bincode::deserialize(&trace.snapshot).unwrap();
            for index in 0..60 {
                state = trace.replay_step(&state, index).unwrap().unwrap();
                assert_eq!(state.fighters[1].damage, if grounded { 10.0 } else { 0.0 });
                if index == 0 {
                    assert_eq!(state.fighters[0].state, super::sim::CharState::SpecialLandD);
                    assert_eq!(state.fighters[0].frame, 0);
                }
            }
            assert_eq!(state.fighters[0].state, super::sim::CharState::Stand);
            assert!(trace.replay_step(&state, 60).unwrap().is_none());
            assert!(trace.replay_step(&state, 0).is_err());
        }
    }

    use super::*;

    #[test]
    fn cell_trace_replays_pickup_throw_contact_and_eof() {
        let trace = Trace::cells();
        assert_eq!(trace.verify(), Ok(240));
        let mut state: SimState = bincode::deserialize(&trace.snapshot).unwrap();
        for index in 0..240 {
            state = trace.replay_step(&state, index).unwrap().unwrap();
            if index == 159 {
                let slot = usize::try_from(state.fighters[0].holding).unwrap();
                assert_eq!(state.items[slot].kind, sim::ItemKind::TerrainCell);
            }
            if index == 162 { assert!(state.items.iter().any(|i| i.cell.is_some() && i.thrown)); }
            if index == 163 {
                assert!(!state.items.iter().any(|i| i.cell.is_some()));
                assert_eq!(state.paths.iter().filter_map(|p| p.cell.map(|c| (c.id.get(), p.percent)))
                    .collect::<Vec<_>>(), vec![(18, trace.tune.throw_item.hit.damage), (19, 0.0), (20, 0.0)]);
            }
        }
        assert!(trace.replay_step(&state, 240).unwrap().is_none());
    }

    #[test]
    fn falcon_dive_inputs_catch_or_whiff_and_replay_through_landing() {
        use sim::CharState;
        for catch in [true, false] {
            let trace = Trace::falcon_dive(if catch { DiveStart::Catch } else { DiveStart::Whiff });
            assert_eq!(trace.verify(), Ok(180));
            let mut state: SimState = bincode::deserialize(&trace.snapshot).unwrap();
            let ground_y = state.fighters[0].pos.y;
            let mut states = Vec::new();
            let mut rose = false;
            for index in 0..180 {
                state = trace.replay_step(&state, index).unwrap().unwrap();
                let current = state.fighters[0].state;
                if states.last() != Some(&current) { states.push(current); }
                rose |= state.fighters[0].pos.y < ground_y - 20.0;
            }
            assert!(rose, "catch={catch}, states={states:?}, final_y={}, damage={}", state.fighters[0].pos.y, state.fighters[1].damage);
            assert_eq!(states[0], CharState::SpecialU);
            assert_eq!(states.contains(&CharState::GrabHold), catch);
            assert_eq!(states, if catch {
                vec![CharState::SpecialU, CharState::GrabHold, CharState::Air, CharState::Landing, CharState::Stand]
            } else {
                vec![CharState::SpecialU, CharState::Landing, CharState::Stand]
            });
            assert_eq!(state.fighters[1].damage, if catch { 18.0 } else { 0.0 });
            assert_eq!(state.fighters[0].state, CharState::Stand);
            assert!(trace.replay_step(&state, 180).unwrap().is_none());
        }
    }

    #[test]
    fn ship_dive_fixture_rides_then_launches_and_replays() {
        let trace = Trace::falcon_dive(DiveStart::Ship);
        assert_eq!(trace.verify(), Ok(180));
        let mut state: SimState = bincode::deserialize(&trace.snapshot).unwrap();
        for index in 0..180 {
            state = trace.replay_step(&state, index).unwrap().unwrap();
            let fighter = &state.fighters[0];
            if index <= 10 {
                assert_eq!(fighter.state, sim::CharState::SpecialU);
                assert_eq!(fighter.ground_ink, sim::SHIP_SLOT as i8);
                let relative = fighter.pos - state.paths[sim::SHIP_SLOT].pos;
                assert!((relative - sim::Vector2::new(0.0, sim::SHIP_R)).length() < 0.1);
            }
            if index == 11 { assert!(!fighter.grounded()); assert!(fighter.vel.y < 0.0); }
        }
        assert!(trace.replay_step(&state, 180).unwrap().is_none());
    }

    #[test]
    fn jump_grab_fixture_steps_and_rejects_external_state_edits() {
        let trace = Trace::jump_grab();
        assert_eq!(trace.verify(), Ok(45));
        let initial: SimState = bincode::deserialize(&trace.snapshot).unwrap();
        let jump = trace.replay_step(&initial, 0).unwrap().unwrap();
        assert_eq!(jump.fighters[0].state, sim::CharState::JumpSquat);
        let grab = trace.replay_step(&jump, 1).unwrap().unwrap();
        assert_eq!(grab.fighters[0].state, sim::CharState::Grab);
        assert_eq!(grab.fighters[0].pos.y, initial.fighters[0].pos.y);
        let mut state = grab;
        for index in 2..trace.frames.len() { state = trace.replay_step(&state, index).unwrap().unwrap(); }
        assert!(trace.replay_step(&state, 45).unwrap().is_none());
        state.fighters[0].damage += 1.0;
        assert_eq!(trace.replay_step(&state, 0).err().as_deref(), Some("Replay state changed; restore start first."));
    }

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
        let initial = bincode::deserialize(&trace.snapshot).unwrap();
        assert!(trace.replay_step(&initial, 0).unwrap().is_some());
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
    replay_cursor: usize,
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
                        ("Restore start", Command::Restore), ("Replay step", Command::ReplayStep),
                        ("Load fixture", Command::Fixture), ("Falcon jump-grab", Command::JumpGrab),
                        ("Dive catch", Command::Dive(DiveStart::Catch)), ("Dive whiff", Command::Dive(DiveStart::Whiff)),
                        ("Ship Dive", Command::Dive(DiveStart::Ship)), ("Cell replay", Command::CellReplay),
                        ("Kick ground", Command::KickContact(true)), ("Kick air", Command::KickContact(false)),
                        ("Travel ground", Command::KickTravel(false)), ("Travel air", Command::KickTravel(true)),
                        ("Fair early", Command::FairContact(false)), ("Fair late", Command::FairContact(true))] {
                        if ui.button(label).clicked() { self.command = Some(command); }
                    }
                });
            });
            if !offline { ui.label("Controls are local-only. Network simulation continues normally."); }
            if self.isolated { ui.label("Debug state: automatic world saving disabled until scene reload."); }
            if let Some(trace) = &self.trace { ui.label(format!("{} recorded ticks; {}", trace.frames.len(),
                if trace.recording { "recording" } else { "stopped" })); }
            ui.label(&self.message);
            if self.trace.is_some() { ui.label(format!("Replay position: {}", self.replay_cursor)); }
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
                self.debugger.replay_cursor = 0;
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
                        self.charsel.set([state.fighters[0].char_id as i64, state.fighters[1].char_id as i64]);
                        self.state.set(state);
                        self.tune.set(trace.tune.clone());
                        self.debugger.paused = true;
                        self.debugger.isolated = true;
                        self.debugger.history.clear();
                        self.debugger.selected = None;
                        self.debugger.replay_cursor = 0;
                    }
                }
            }
            Some(Command::Fixture) => {
                self.state.set(sim::terrain_cells::playground());
                self.charsel.set([2, 3]);
                self.tune.set(Tune::default());
                self.debugger = Debugger { paused: true, isolated: true,
                    message: "Fixture paused. Resume and strike the blue cells. Dropper left of Falcon; ship farther left.".into(),
                    ..Debugger::default() };
            }
            Some(Command::JumpGrab | Command::Dive(_) | Command::CellReplay | Command::KickContact(_) | Command::KickTravel(_) | Command::FairContact(_)) => {
                let (trace, message) = match command {
                    Some(Command::FairContact(late)) => (Trace::idle(sim::fixtures::fair_contact(late)),
                        "Replay step 1: Falcon forward-air contact. Early: 18 damage; late: 6. Authored geometry/timing mapping; PM parity unverified."),
                    Some(Command::KickTravel(air)) => (Trace::idle(sim::fixtures::kick_travel(air)),
                        "Replay step 1: late Kick travel. Ground entry: 9 damage; air entry: 11. Authored timing; PM parity unverified."),
                    Some(Command::KickContact(grounded)) => (Trace::idle(sim::fixtures::kick_contact(grounded)),
                        "Replay step 1: Kick landing contact. Ground target: 10 damage; air target: 0. Authored timing; PM parity unverified."),
                    Some(Command::CellReplay) => (Trace::cells(),
                        "Replay step: 91 = strike, 141 = pickup, 163 = throw, 164 = cell impact. Game3 item rules."),
                    Some(Command::Dive(catch)) => (Trace::falcon_dive(catch),
                        "Replay step: Falcon Dive startup, rise, catch/explosion or whiff, landing. Game3 rules; PM parity unverified."),
                    _ => (Trace::jump_grab(),
                        "Replay step: 1 = jump, 2 = grounded grab. Provisional Game3 rules; PM parity unverified."),
                };
                let state: SimState = bincode::deserialize(&trace.snapshot).unwrap();
                self.charsel.set([state.fighters[0].char_id as i64, state.fighters[1].char_id as i64]);
                self.state.set(state);
                self.tune.set(trace.tune.clone());
                self.debugger = Debugger { paused: true, isolated: true, trace: Some(trace),
                    message: message.into(),
                    ..Debugger::default() };
            }
            Some(Command::ReplayStep) => {
                self.debugger.paused = true;
                if let Some(trace) = &mut self.debugger.trace {
                    trace.recording = false;
                    match trace.replay_step(&self.state.get(), self.debugger.replay_cursor) {
                        Ok(Some(next)) => {
                            self.state.set(next);
                            self.tune.set(trace.tune.clone());
                            self.debugger.replay_cursor += 1;
                            self.debugger.isolated = true;
                        }
                        Ok(None) => self.debugger.message = "Replay finished.".into(),
                        Err(error) => self.debugger.message = error,
                    }
                }
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
