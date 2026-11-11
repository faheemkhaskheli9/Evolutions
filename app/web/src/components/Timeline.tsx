import { useEffect, useState } from "react";
import type { CheckpointInfo, CheckpointReason } from "../engine/checkpoints";
import type { BranchInfo } from "../engine/branches";

const REASONS: Record<CheckpointReason, string> = {
  start: "start",
  periodic: "checkpoint",
  jump: "before a jump",
  intervention: "before an intervention",
  branch: "branch point",
};

export const checkpointYear = (y: number) => (y < 10 ? y.toFixed(2) : Math.floor(y).toLocaleString());

/** Where a checkpoint sits on the strip, in percent: from the first checkpoint to now. */
export function timelinePos(year: number, from: number, to: number): number {
  return to > from ? Math.max(0, Math.min(100, (100 * (year - from)) / (to - from))) : 100;
}

/** The branch switcher (PRD FR-T7): pick, start, rename, remove and compare branches. */
function Branches({
  branches,
  disabled,
  onBranch,
  onSwitch,
  onRename,
  onRemove,
  onCompare,
}: {
  branches: BranchInfo[];
  disabled: boolean;
  onBranch: () => void;
  onSwitch: (id: number) => void;
  onRename: (id: number, name: string) => void;
  onRemove: (id: number) => void;
  onCompare: () => void;
}) {
  const active = branches.find((b) => b.active);
  if (!active) return null;
  return (
    <span className="branches row gap">
      {branches.length > 1 && (
        <select value={active.id} disabled={disabled} onChange={(e) => onSwitch(Number(e.target.value))} aria-label="Branch" title="Switch to another branch of this world's history">
          {branches.map((b) => (
            <option key={b.id} value={b.id}>
              {b.name}
              {b.parent !== null ? ` (from yr ${checkpointYear(b.fromYear)})` : ""}
            </option>
          ))}
        </select>
      )}
      <button disabled={disabled} onClick={onBranch} title="Start a new branch from the present; this branch is kept as it is">
        ⑂ Branch
      </button>
      {branches.length > 1 && (
        <>
          <button
            className="ghost"
            disabled={disabled}
            aria-label="Rename branch"
            title="Rename this branch"
            onClick={() => {
              const name = window.prompt("Branch name", active.name);
              if (name) onRename(active.id, name);
            }}
          >
            ✎
          </button>
          <select
            value=""
            disabled={disabled}
            aria-label="Remove a branch"
            title="Remove another branch (the one shown can't be removed)"
            onChange={(e) => {
              const b = branches.find((x) => x.id === Number(e.target.value));
              if (b && window.confirm(`Remove the branch "${b.name}"? Its history can't be recovered.`)) onRemove(b.id);
            }}
          >
            <option value="">Remove…</option>
            {branches
              .filter((b) => !b.active)
              .map((b) => (
                <option key={b.id} value={b.id}>
                  {b.name}
                </option>
              ))}
          </select>
          <button disabled={disabled} onClick={onCompare}>
            Compare…
          </button>
        </>
      )}
    </span>
  );
}

/**
 * The checkpoint strip under the time bar (PRD FR-T6): click a mark to rewind to it or to
 * start a branch from it (FR-T7).
 */
export function Timeline({
  checkpoints,
  bytes,
  budget,
  year,
  disabled,
  onRewind,
  branches,
  onBranch,
  onSwitch,
  onRename,
  onRemove,
  onCompare,
}: {
  checkpoints: CheckpointInfo[];
  bytes: number;
  budget: number;
  year: number;
  disabled: boolean;
  onRewind: (id: number) => void;
  branches: BranchInfo[];
  /** Starts a branch at checkpoint `from`, or at the present when null. */
  onBranch: (from: number | null) => void;
  onSwitch: (id: number) => void;
  onRename: (id: number, name: string) => void;
  onRemove: (id: number) => void;
  onCompare: () => void;
}) {
  const [open, setOpen] = useState<number | null>(null);
  useEffect(() => {
    if (open !== null && !checkpoints.some((c) => c.id === open)) setOpen(null);
  }, [checkpoints, open]);
  if (!checkpoints.length) return null;
  const from = checkpoints[0].year;
  const to = Math.max(year, checkpoints[checkpoints.length - 1].year);
  const mb = (b: number) => `${(b / (1024 * 1024)).toFixed(b < 10 * 1024 * 1024 ? 1 : 0)} MB`;
  const sel = checkpoints.find((c) => c.id === open);
  return (
    <div className="timeline">
      <span className="muted" title="Checkpoints are kept every 10 simulated years, before each jump and before each intervention; older ones are thinned out. Click one to rewind or branch from it.">
        Rewind
      </span>
      <div className="timeline-track" role="group" aria-label="Checkpoints">
        <div className="timeline-now" style={{ left: `${timelinePos(year, from, to)}%` }} />
        {checkpoints.map((c) => {
          const label = `Checkpoint, year ${checkpointYear(c.year)} (${REASONS[c.reason]})`;
          return (
            <button
              key={c.id}
              className={`timeline-mark mark-${c.reason} ${c.id === open ? "open" : ""}`}
              style={{ left: `${timelinePos(c.year, from, to)}%` }}
              disabled={disabled}
              aria-label={label}
              aria-expanded={c.id === open}
              title={label}
              onClick={() => setOpen(c.id === open ? null : c.id)}
            />
          );
        })}
        {sel && (
          <div className="timeline-menu" role="menu" style={{ left: `${timelinePos(sel.year, from, to)}%` }}>
            <b className="small">Year {checkpointYear(sel.year)}</b>
            <button
              role="menuitem"
              disabled={disabled}
              title="Go back to this checkpoint; what happened after it in this branch is discarded"
              onClick={() => {
                setOpen(null);
                onRewind(sel.id);
              }}
            >
              Rewind here
            </button>
            <button
              role="menuitem"
              disabled={disabled}
              title="Start a new branch from this checkpoint; this branch is kept as it is"
              onClick={() => {
                setOpen(null);
                onBranch(sel.id);
              }}
            >
              ⑂ Branch from here
            </button>
          </div>
        )}
      </div>
      <span className="timeline-meta muted" title={`Checkpoints use at most ${mb(budget)} of memory per branch`}>
        {checkpoints.length} · {mb(bytes)}
      </span>
      <Branches branches={branches} disabled={disabled} onBranch={() => onBranch(null)} onSwitch={onSwitch} onRename={onRename} onRemove={onRemove} onCompare={onCompare} />
    </div>
  );
}
