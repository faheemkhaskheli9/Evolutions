import { useEffect, useState } from "react";

interface Props extends Omit<React.InputHTMLAttributes<HTMLInputElement>, "value" | "onChange" | "min" | "max"> {
  value: number;
  min: number;
  max: number;
  onChange: (v: number) => void;
}

/**
 * Number box that lets the user type freely and clamps to [min, max] only on blur or
 * Enter; clamping on every keystroke turns "100" → backspace into "2".
 */
export function NumberInput({ value, min, max, onChange, ...rest }: Props) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);
  const commit = () => {
    const n = Number(text);
    const v = text.trim() === "" || !isFinite(n) ? value : Math.round(Math.max(min, Math.min(max, n)));
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
