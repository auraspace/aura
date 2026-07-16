interface SearchBoxProps {
  value: string
  onChange: (v: string) => void
}

const fieldClass =
  'min-w-40 rounded-[0.4rem] border border-border bg-card px-2.5 py-1.5 text-fg'

export function SearchBox({ value, onChange }: SearchBoxProps) {
  return (
    <label className="flex flex-col gap-1 text-xs text-muted">
      Search
      <input
        type="search"
        className={fieldClass}
        placeholder="Title, status, body…"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  )
}
