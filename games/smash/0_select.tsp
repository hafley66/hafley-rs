namespace Smash;

// Stable identities. Scalars, not wrapper models.
scalar InputDeviceId extends string;
scalar PlayerSlotId extends string;
scalar CharacterId extends string;

// The runtime roster, authored as identities rather than an enum value table.
const roster: CharacterId[] = #["pigeon", "dog"];

// Selection events. One shape per event; `kind` is the transport discriminant.
model JoinEvent {
  kind: "join";
  device: InputDeviceId;
  slot: PlayerSlotId;
}

model LeaveEvent {
  kind: "leave";
  slot: PlayerSlotId;
}

model ChooseEvent {
  kind: "choose";
  slot: PlayerSlotId;
  character: CharacterId;
}

model ReadyEvent {
  kind: "ready";
  slot: PlayerSlotId;
}

model UnreadyEvent {
  kind: "unready";
  slot: PlayerSlotId;
}

alias SelectionEvent = JoinEvent | LeaveEvent | ChooseEvent | ReadyEvent | UnreadyEvent;

// One record per occupied slot. The slot id is the record key; device
// ownership, chosen character and ready status appear exactly once.
model SlotSelection {
  device: InputDeviceId;
  character?: CharacterId;
  ready: boolean;
}

model SelectionState {
  slots: Record<SlotSelection>;
}

// Start eligibility is derived in Rust from SelectionState, never authored.

const events: SelectionEvent[] = #[
  #{ kind: "join", device: "pad-0", slot: "p1" },
  #{ kind: "choose", slot: "p1", character: "pigeon" },
  #{ kind: "ready", slot: "p1" },
  #{ kind: "unready", slot: "p1" },
  #{ kind: "leave", slot: "p1" },
];

const selection: SelectionState = #{
  slots: #{
    p1: #{ device: "pad-0", character: "pigeon", ready: true },
    p2: #{ device: "keyboard", character: "dog", ready: false },
  },
};
