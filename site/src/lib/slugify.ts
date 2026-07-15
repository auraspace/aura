export function slugify(text: string): string {
  return text
    .toLowerCase()
    .trim()
    .replace(/&/g, ' ')
    .replace(/[^\w\s-]/g, '')
    .replace(/[\s_]+/g, '-')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '')
}
