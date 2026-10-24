import type { CheckpointInfo, CheckpointReason } from "../engine/checkpoints";

const REASONS: Record<CheckpointReason, string> = {
  start: "start",
  periodic: "checkpoint",
  jump: "before a jump",
  intervention: "before an intervention",
};

export const checkpointYear = (y: number) => (y < 10 ? y.toFixed(2) : Math.floor(y).toLocaleString());

/** Where a checkpoint sits on the strip, in percent: from the first checkpoint to now. */
export function timelinePos(year: number, from: number, to: number): number {
  return to > from ? Math.max(0, Math.min(100, (100 * (year - from)) / (to - from))) : 100;
}

/** The checkpoint strip under the time bar (PRD FR-T6): click a mark to rewind to it. */
export function Timeline({
  checkpoints,
  bytes,
  budget,
  year,
  disabled,
  onRewind,
}: {
  checkpoints: CheckpointInfo[];
  bytes: number;
  budget: number;
  year: number;
  disabled: boolean;
  onRewind: (id: number) => void;
}) {
  if (!checkpoints.length) return null;
  const from = checkpoints[0].year;
  const to = Math.max(year, checkpoints[checkpoints.length - 1].year);
  const mb = (b: number) => `${(b / (1024 * 1024)).toFixed(b < 10 * 1024 * 1024 ? 1 : 0)} MB`;
  return (
    <div className="timeline">
      <span className="muted" title="Checkpoints are kept every 10 simulated years, before each jump and before each intervention; older ones are thinned out.">
        Rewind
      </span>
      <div className="timeline-track" role="group" aria-label="Checkpoints">
        <div className="timeline-now" style={{ left: `${timelinePos(year, from, to)}%` }} />
        {checkpoints.map((c) => {
          const label = `Rewind to year ${checkpointYear(c.year)} (${REASONS[c.reason]})`;
          return (
            <button
              key={c.id}
              className={`timeline-mark mark-${c.reason}`}
              style={{ left: `${timelinePos(c.year, from, to)}%` }}
              disabled={disabled}
              aria-label={label}
              title={`${label}. What happened after it is discarded.`}
              onClick={() => onRewind(c.id)}
            />
          );
        })}
      </div>
      <span className="timeline-meta muted" title={`Checkpoints use at most ${mb(budget)} of memory`}>
        {checkpoints.length} · {mb(bytes)}
      </span>
    </div>
  );
}
