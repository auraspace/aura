export type { GuideDoc, GuideHeading, GuideMeta, GuideNavSection } from './types'
export { parseFrontmatter, parseGuideMarkdown, stripLeadingH1 } from './parse-guide'
export {
  getAdjacentGuides,
  getAllGuideMeta,
  getAllGuides,
  getGuideBySlug,
  getGuideNav,
  loadAllGuides,
} from './load-guides'
export {
  buildGuideSearchIndex,
  searchGuides,
  type GuideSearchHit,
} from './search'
