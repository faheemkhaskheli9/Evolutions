import { useState } from "react";

interface Props {
  initialName: string;
  initialDescription: string;
  onSave: (name: string, description: string) => void;
  onCancel: () => void;
}

/** Name and description for a new user template (PRD FR-W9). */
export function TemplateForm({ initialName, initialDescription, onSave, onCancel }: Props) {
  const [name, setName] = useState(initialName);
  const [description, setDescription] = useState(initialDescription);
  const ok = name.trim().length > 0;
  return (
    <form
      className="template-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (ok) onSave(name.trim(), description.trim());
      }}
    >
      <label className="setting">
        <span>Template name</span>
        <input
          value={name}
          maxLength={80}
          onChange={(e) => setName(e.target.value)}
          aria-label="Template name"
          placeholder="e.g. Lesson 3: island dwarfism"
          autoFocus
        />
      </label>
      <label className="setting">
        <span>Description</span>
        <textarea
          value={description}
          maxLength={2000}
          rows={2}
          onChange={(e) => setDescription(e.target.value)}
          aria-label="Template description"
          placeholder="What to look for (optional)"
        />
      </label>
      <div className="row gap">
        <button type="submit" className="primary" disabled={!ok}>
          Save template
        </button>
        <button type="button" className="ghost" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </form>
  );
}
