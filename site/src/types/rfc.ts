export type RfcStatus =
  | 'Draft'
  | 'In Review'
  | 'Accepted'
  | 'Frozen'
  | 'Rejected'
  | 'Superseded'

export interface RfcMeta {
  id: string
  slug: string
  title: string
  status: RfcStatus
  layer: string
  authors: string[]
  created?: string
  updated?: string
  estimate?: string
  depends: string[]
  blocks: string[]
  fileName: string
}

export interface Heading {
  depth: number
  text: string
  id: string
}

export interface RfcDoc extends RfcMeta {
  markdown: string
  headings: Heading[]
}
