import "./0_model.tsp";
using Games;

// Authored status identity only. `8_status.mjs` derives every observed value and
// rejects any authored identity that the generated catalog, the retained
// manifest or the executable export cannot satisfy. Do not add observed values
// here.
//
// Pigeon authors `expected` because its family names drive fidelity disposition
// and its stable IDs are the join key for the live selection export. Dog leaves
// `expected` empty: its generated catalog (`generated/0_catalog.json`) owns the
// ordered inventory, so no action list is restated in TypeSpec.
const status: Status = #{
  characters: #{
    pigeon: #{
      runtime: "pigeon",
      display_name: "Private Pigeon",
      catalog: "smash/src/fighters/pigeon/generated/5_catalog.json",
      manifest: "smash/src/fighters/pigeon/imported/0_sources.json",
      baked: "smash/src/fighters/pigeon/generated/6_baked.json",
      axes: #["payload", "catalog", "phase", "chart", "live", "restore", "native", "fidelity"],
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
    },
    dog: #{
      runtime: "dog",
      display_name: "Dog",
      catalog: "smash/src/fighters/dog/generated/0_catalog.json",
      manifest: "smash/src/fighters/dog/imported/0_sources.json",
      baked: "smash/src/fighters/dog/generated/1_baked.json",
      axes: #["payload", "catalog", "phase", "chart", "live", "restore", "native", "fidelity"],
      expected: #{},
    },
  },
};
