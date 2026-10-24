import { describe, expect, it } from "vitest";
import { describeSpeed, fmt, formatDate, formatYears, ramp, smoothSpeed, type RGB } from "./util";

describe("formatYears", () => {
  it("picks a unit by size", () => {
    expect(formatYears(0.5)).toBe("182 days");
    expect(formatYears(12.34)).toBe("12.3 years");
    expect(formatYears(250)).toBe("250 years");
    expect(formatYears(12_500)).toBe("12.5 thousand years");
    expect(formatYears(3_400_000)).toBe("3.40 million years");
  });
});

describe("formatDate", () => {
  it("shows the year, month, day and hour", () => {
    expect(formatDate(12.7, 0, 5)).toBe("Year 12 · Jan 1 · 05:00");
    expect(formatDate(3, 45, 18)).toBe("Year 3 · Feb 15 · 18:00");
  });
  it("keeps the last days of the year in December", () => {
    expect(formatDate(1, 365, 0)).toMatch(/Dec/);
  });
});

describe("fmt", () => {
  it("uses fewer decimals for bigger numbers", () => {
    expect(fmt(1.2345)).toBe("1.23");
    expect(fmt(12.345)).toBe("12.3");
    expect(fmt(123.45)).toBe("123");
    expect(fmt(12345.6)).toBe((12346).toLocaleString());
    expect(fmt(-0.5, 1)).toBe("-0.5");
  });
  it("shows a dash for non-finite values", () => {
    expect(fmt(NaN)).toBe("–");
    expect(fmt(Infinity)).toBe("–");
  });
});

describe("describeSpeed", () => {
  it("uses hours, days or years per second", () => {
    expect(describeSpeed(3600)).toBe("1.0 h/s");
    expect(describeSpeed(7 * 86400)).toBe("7.0 days/s");
    expect(describeSpeed(10 * 365.25 * 86400)).toBe("10.0 years/s");
  });
});

describe("ramp", () => {
  const stops: RGB[] = [
    [0, 0, 0],
    [100, 200, 50],
    [200, 0, 250],
  ];
  it("interpolates between stops", () => {
    expect(ramp(stops, 0)).toEqual([0, 0, 0]);
    expect(ramp(stops, 0.25)).toEqual([50, 100, 25]);
    expect(ramp(stops, 0.5)).toEqual([100, 200, 50]);
    expect(ramp(stops, 1)).toEqual([200, 0, 250]);
  });
  it("clamps t to 0–1", () => {
    expect(ramp(stops, -3)).toEqual([0, 0, 0]);
    expect(ramp(stops, 7)).toEqual([200, 0, 250]);
  });
});

describe("smoothSpeed", () => {
  const year = 365.25 * 86400;

  // Phase 5 web bug 4: in Population mode a one-year step lands on only some frames, and
  // the label flickered between 0 and a spike. It now follows the requested speed.
  it("stays at the requested speed when nothing is dropped", () => {
    let a = 0;
    for (let i = 0; i < 120; i++) a = smoothSpeed(a, 10 * year, 0, 1 / 60);
    expect(a / (10 * year)).toBeCloseTo(0.86, 1);
    for (let i = 0; i < 600; i++) a = smoothSpeed(a, 10 * year, 0, 1 / 60);
    expect(a / (10 * year)).toBeCloseTo(1, 3);
  });
  it("subtracts the backlog dropped, never below zero", () => {
    expect(smoothSpeed(0, 86400, 60 * 12, 1)).toBe(86400 - 12 * 3600);
    expect(smoothSpeed(0, 86400, 1e9, 1)).toBe(0);
  });
  it("ignores frames with no elapsed time", () => {
    expect(smoothSpeed(42, 86400, 0, 0)).toBe(42);
  });
});
