import { useEffect, useState } from "react";

interface Props extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "min" | "max"> {
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
}

/** The whole number typed in `text`, clamped to [min, max]; `fallback` if it isn't a number. */
export function parseClamped(text: string, fallback: number, min: number, max: number): number {
  const n = Number(text);
  return text.trim() === "" || !isFinite(n) ? fallback : Math.round(Math.max(min, Math.min(max, n)));
}

/**
 * Number box that lets the user type freely and clamps to [min, max] only on blur or
 * Enter; clamping on every keystroke turns "100" → backspace into "2".
 */
export function NumberInput({ value, min, max, onChange, ...rest }: Props) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const v = parseClamped(text, value, min, max);
    setText(String(v));
    if (v !== value) onChange(v);
  };
  return (
    <input
      {...rest}
      type="number"
      min={min}
      max={max}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
    />
  );
}
