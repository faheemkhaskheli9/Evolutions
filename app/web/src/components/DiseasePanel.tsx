import { useEffect, useState } from "react";
import type { Disease, SimEvent } from "../engine/protocol";
import { chartMarkers } from "./chartMarkers";
import { count, defaultDiseaseSpecies, diseaseCharts, escapes, LOCUS_COLORS, percent } from "./disease";
import { LineChart } from "./LineChart";

/** Pathogens and host resistance (PRD FR-L12, FR-G14): who is infected, how resistance and the strains evolve. */
export function DiseasePanel({ disease, events }: { disease: Disease; events: SimEvent[] }) {
  const [pick, setPick] = useState<number | null>(null);
  const shown = disease.species.find((s) => s.id === pick) ?? disease.species.find((s) => s.id === defaultDiseaseSpecies(disease));
  useEffect(() => {
    if (pick !== null && !disease.species.some((s) => s.id === pick)) setPick(null);
  }, [disease, pick]);
  const markers = chartMarkers(events);
  const charts = diseaseCharts(shown ? disease.series[String(shown.id)] : undefined);
  const strains = disease.strains.filter((s) => !shown || s.host === shown.id).slice(0, 8);
  const circulating = disease.lineages.filter((l) => l.circulating);
  return (
    <section className="disease" aria-label="Disease">
      <h3>Disease</h3>
      <p className="muted small">
        {circulating.length
          ? `Circulating: ${circulating.map((l) => l.name).join(", ")}.`
          : `No pathogen circulates now (${disease.lineages.map((l) => l.name).join(", ")} died out).`}{" "}
        {count(disease.cases)} infections and {count(disease.deaths)} deaths so far.
      </p>
      {disease.species.length > 1 && (
        <label className="row gap small">
          Species
          <select value={shown?.id ?? ""} onChange={(e) => setPick(Number(e.target.value))} aria-label="Disease species">
            {disease.species.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {shown && (
        <div className="climate-now">
          <div>
            <span className="muted small">Infected</span>
            <b>
              {count(shown.infected)} ({percent(shown.infected, shown.n)})
            </b>
          </div>
          <div>
            <span className="muted small">Immune</span>
            <b>{percent(shown.recovered, shown.n)}</b>
          </div>
          <div>
            <span className="muted small">Deaths from disease</span>
            <b>{count(shown.deaths)}</b>
          </div>
          <div>
            <span className="muted small">Mean virulence</span>
            <b>{shown.virulence >= 0 ? shown.virulence.toFixed(2) : "–"}</b>
          </div>
        </div>
      )}
      {charts.prevalence.length > 1 && (
        <>
          <LineChart yLabel="Infected at the peak (% of hosts)" series={[{ name: "Infected", color: "var(--disease)", points: charts.prevalence }]} markers={markers} />
          <LineChart
            yLabel="Resistant allele frequency"
            series={charts.resistance.map((points, i) => ({ name: `R${i + 1}`, color: LOCUS_COLORS[i], points }))}
            markers={markers}
          />
          {charts.virulence.length > 1 && (
            <LineChart
              yLabel="Strains infecting it (mean)"
              series={[
                { name: "Virulence", color: "var(--disease)", points: charts.virulence },
                { name: "Transmissibility", color: "#7fb2ff", points: charts.transmissibility },
              ]}
              markers={markers}
            />
          )}
        </>
      )}
      {strains.length > 0 && (
        <>
          <h4>Strains</h4>
          <table className="strains small">
            <thead>
              <tr>
                <th>Strain</th>
                <th title="Death rate while infected (0–1)">Virulence</th>
                <th title="Transmission per contact (0–1)">Transm.</th>
                <th title="Share of infections that end in death">Fatal</th>
                <th title="Basic reproduction number in its host now">R₀</th>
                <th>Infected</th>
              </tr>
            </thead>
            <tbody>
              {strains.map((s) => (
                <tr key={s.id} title={`${escapes(s)}${s.trade_off ? ", with the trade-off" : ""}. Since year ${count(s.origin_year)}: ${count(s.cases)} cases, ${count(s.deaths)} deaths.`}>
                  <td>{s.name}</td>
                  <td>{s.virulence.toFixed(2)}</td>
                  <td>{s.transmissibility.toFixed(2)}</td>
                  <td>{Math.round(100 * s.fatality)}%</td>
                  <td>{s.r0.toFixed(1)}</td>
                  <td>{count(s.infected)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </>
      )}
    </section>
  );
}
