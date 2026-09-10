import "./0_model.tsp";
using Games;

// Authored status identity only: stable expected catalog IDs, the action names
// the executable export must match, their mechanic family for fidelity
// disposition, and the required observation axes. `8_status.mjs` derives
// everything else and rejects any of these that the export or manifest cannot
// satisfy. Do not add observed values here.
const status: Status = #{
  expected: #{
    Wait1: #{ id: 0, action: "Wait1", family: "Ground movement" },
    JumpF: #{ id: 1, action: "JumpF", family: "Jump and fall" },
    AttackAirF: #{ id: 2, action: "AttackAirF", family: "Aerial attacks" },
    JumpSquat: #{ id: 3, action: "JumpSquat", family: "Jump and fall" },
    Fall: #{ id: 4, action: "Fall", family: "Jump and fall" },
    LandingAirF: #{ id: 5, action: "LandingAirF", family: "Aerial attacks" },
    LandingHeavy: #{ id: 6, action: "LandingHeavy", family: "Aerial attacks" },
    WalkSlow: #{ id: 7, action: "WalkSlow", family: "Ground movement" },
    WalkMiddle: #{ id: 8, action: "WalkMiddle", family: "Ground movement" },
    WalkFast: #{ id: 9, action: "WalkFast", family: "Ground movement" },
    Dash: #{ id: 10, action: "Dash", family: "Ground movement" },
    Run: #{ id: 11, action: "Run", family: "Ground movement" },
    RunBrake: #{ id: 12, action: "RunBrake", family: "Ground movement" },
    Turn: #{ id: 13, action: "Turn", family: "Ground movement" },
    TurnRun: #{ id: 14, action: "TurnRun", family: "Ground movement" },
    JumpB: #{ id: 15, action: "JumpB", family: "Jump and fall" },
    JumpAerialF: #{ id: 16, action: "JumpAerialF", family: "Jump and fall" },
    LandingLight: #{ id: 17, action: "LandingLight", family: "Aerial attacks" },
    Squat: #{ id: 18, action: "Squat", family: "Crouch" },
    SquatWait: #{ id: 19, action: "SquatWait", family: "Crouch" },
    SquatRv: #{ id: 20, action: "SquatRv", family: "Crouch" },
    JumpAerialB: #{ id: 21, action: "JumpAerialB", family: "Jump and fall" },
  },
  axes: #["payload", "catalog", "phase", "chart", "live", "restore", "native", "fidelity"],
};
