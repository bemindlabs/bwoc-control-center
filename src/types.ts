export type AgentEntry = {
  id: string;
  path: string;
  backend: string;
  incarnated: string;
  status: string;
  running?: boolean;
  inbox_count?: number;
  uptime_seconds?: number | null;
};

/** One Aparihāniya health condition from `bwoc fleet health --json`. */
export type HealthSignal = {
  condition: number;
  name: string;
  finding: string;
  status: string; // "ok" | "info" | "warn"
};

export type FleetSnapshot = {
  workspace: string;
  /** `bwoc list --json` `.agents` — array of registry entries. */
  agents: AgentEntry[] | null;
  /** `bwoc fleet health --json` — array of signals, or null when unavailable. */
  health: HealthSignal[] | null;
};

export type HostInfo = {
  hostname: string;
  workspace: string;
  version: string | null;
};

export type HostListEntry = {
  name: string;
  url: string | null;
};

export type ControllerInfo = {
  id: string;
  public_key: string;
};
