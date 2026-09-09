namespace Games;

// Project adaptation of TC39 stages. Advancement is an explicit decision.
alias Stage = 0 | 1 | 2 | 2.7 | 3 | 4;
alias Destination = "library" | "app" | "split" | "undecided";

model Entry {
  stage: Stage;
  destination: Destination;
  scope: string;
  task: string;
  // Existing code uses the Cargo package name as registry key.
  // Proposals may omit manifest only at stages 0 or 1.
  manifest?: string;
  targets: string[];
  evidence: string[];
}

alias Registry = Record<Entry>;
