import "./0_model.tsp";
using Games;

// Authored profile, explicit source-to-runtime state mappings and bounded
// exclusions. Coverage values are observations emitted by 15_coverage.mjs.
const port: Port = #{
  profile: #{
    id: "pigeon-falcon-gameplay",
    name: "Pigeon/Falcon-like fighter gameplay",
    target: "smash",
    sourceInventory: "classification/12_source_inventory.json",
    runtimeInventory: "classification/14_runtime_inventory.json",
    sourceRevision: "c7861544f8e1fbc530612393e91d859886e97e3c",
    axes: #["state mapping", "transition/callback mapping", "ordered guard qualification", "live reachability", "rollback", "source fidelity"],
  },
  mappings: #[
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_Wait:14", runtimeState: "Idle", reason: "Runtime Idle is the neutral grounded phase for source Wait.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_WalkSlow:15", runtimeState: "Walk", reason: "Runtime Walk covers the source walk speed bands.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_WalkMiddle:16", runtimeState: "Walk", reason: "Runtime Walk covers the source walk speed bands.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_WalkFast:17", runtimeState: "Walk", reason: "Runtime Walk covers the source walk speed bands.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_Dash:20", runtimeState: "Dash", reason: "Runtime Dash is the executable dash phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_Run:21", runtimeState: "Run", reason: "Runtime Run is the executable run phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_RunBrake:23", runtimeState: "Brake", reason: "Runtime Brake is the executable run-brake phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_Turn:18", runtimeState: "Turn", reason: "Runtime Turn is the executable turn phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_Squat:39", runtimeState: "CrouchEnter", reason: "Runtime crouch lifecycle enters from source Squat.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_SquatWait:40", runtimeState: "CrouchHold", reason: "Runtime crouch lifecycle holds source SquatWait.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_SquatRv:41", runtimeState: "CrouchExit", reason: "Runtime crouch lifecycle exits through source SquatRv.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_Landing:42", runtimeState: "Landing", reason: "Runtime Landing is the executable landing phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_JumpF:25", runtimeState: "Jump", reason: "Runtime Jump is the shared jump phase for directional source jumps.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_JumpB:26", runtimeState: "Jump", reason: "Runtime Jump is the shared jump phase for directional source jumps.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_JumpAerialF:27", runtimeState: "AirJump", reason: "Runtime AirJump is the shared aerial-jump phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_JumpAerialB:28", runtimeState: "AirJump", reason: "Runtime AirJump is the shared aerial-jump phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_Fall:29", runtimeState: "Fall", reason: "Runtime Fall is the shared falling phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_FallF:30", runtimeState: "Fall", reason: "Runtime Fall is the shared falling phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_FallB:31", runtimeState: "Fall", reason: "Runtime Fall is the shared falling phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_FallAerial:32", runtimeState: "Fall", reason: "Runtime Fall is the shared falling phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_FallAerialF:33", runtimeState: "Fall", reason: "Runtime Fall is the shared falling phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
    #{ sourceId: "melee-ftcommon:c7861544f8e1fbc530612393e91d859886e97e3c:src/melee/ft/kinds/ftCommon/forward.h:ftCommon_MotionState:ftCo_MS_FallAerialB:34", runtimeState: "Fall", reason: "Runtime Fall is the shared falling phase.", evidence: #["games/classification/12_source_inventory.json", "games/crates/fighter/src/_5_status.rs"] },
  ],
  // Each mapping sourceId will identify one source.associations row. The
  // parent callback identity is a separate field in the mapping contract.
  callbackMappings: #[],
  guardMappings: #[],
  exclusions: #[
    #{ id: "item-prefix", namePrefixes: #["Item"], expectedMatches: 22, reason: "Item-system states are outside the Pigeon/Falcon-like gameplay profile.", evidence: #["games/classification/12_source_inventory.json", "games/4_tasks.md"] },
    #{ id: "item-throw", namePrefixes: #["LightThrow", "HeavyThrow"], expectedMatches: 26, reason: "Item throw states are outside the profile; item ownership is tracked by T3.", evidence: #["games/classification/12_source_inventory.json", "games/4_tasks.md"] },
    #{ id: "item-weapon", namePrefixes: #["SwordSwing", "BatSwing", "ParasolSwing", "HarisenSwing", "StarRodSwing", "LGun", "FireFlower", "Hammer", "Kinoko"], expectedMatches: 41, reason: "Item weapon states are outside the profile; item-system behavior remains an explicit queue item.", evidence: #["games/classification/12_source_inventory.json", "games/4_tasks.md"] },
    #{ id: "item-lift", namePrefixes: #["Lift"], expectedMatches: 4, reason: "Item lift states are outside the profile; item-system behavior remains an explicit queue item.", evidence: #["games/classification/12_source_inventory.json", "games/4_tasks.md"] },
  ],
};
