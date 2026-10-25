import type { Why } from "../engine/protocol";
import { chartLabel, verdictLabel } from "./why";

/** A "Why?" explanation (PRD FR-O8): the answer in plain language, the points and numbers behind it, and a link to the chart that shows it. */
export function WhyPanel({ why, onShowChart }: { why: Why | null | undefined; onShowChart?: (why: Why) => void }) {
  if (why === undefined) return <div className="why muted small">Working it out…</div>;
  if (why === null) return <div className="why muted small">No explanation was recorded for this event (it happened before this world kept them).</div>;
  const verdict = verdictLabel(why.verdict);
  return (
    <div className="why" role="region" aria-label="Why?">
      {verdict && <span className={`why-verdict why-${why.verdict}`}>{verdict}</span>}
      <p>{why.summary}</p>
      {why.points.length > 0 && (
        <ul className="why-points">
          {why.points.map((p, i) => (
            <li key={i}>{p}</li>
          ))}
        </ul>
      )}
      {why.numbers.length > 0 && (
        <details>
          <summary>The numbers</summary>
          <table className="why-numbers">
            <tbody>
              {why.numbers.map(([k, v], i) => (
                <tr key={i}>
                  <th scope="row" className={k.startsWith("  ") ? "sub" : ""}>
                    {k.trim()}
                  </th>
                  <td>{v}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </details>
      )}
      {onShowChart && why.chart && (
        <button className="link" onClick={() => onShowChart(why)}>
          {chartLabel(why.chart)} →
        </button>
      )}
    </div>
  );
}
