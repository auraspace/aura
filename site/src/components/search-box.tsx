interface SearchBoxProps {
  value: string
  onChange: (v: string) => void
}

export function SearchBox({ value, onChange }: SearchBoxProps) {
  return (
    <label>
      Search
      <input
        type="search"
        placeholder="Title, status, body…"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </label>
  )
}
