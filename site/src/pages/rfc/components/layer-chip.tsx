export function LayerChip({ layer }: { layer: string }) {
  return (
    <span className="inline-block rounded-full border border-border bg-card px-2 py-0.5 text-xs text-muted">
      {layer}
    </span>
  )
}
