export interface GuideHeading {
  depth: number
  text: string
  id: string
}

export interface GuideMeta {
  slug: string
  title: string
  section: string
  order: number
  summary: string
  fileName: string
}

export interface GuideDoc extends GuideMeta {
  markdown: string
  headings: GuideHeading[]
}

export interface GuideNavSection {
  title: string
  items: GuideMeta[]
}
