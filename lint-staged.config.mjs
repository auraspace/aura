/** @type {import('lint-staged').Configuration} */
export default {
  // oxlint has no prettier / simple-import-sort — format + sort imports via Prettier.
  'site/**/*.{ts,tsx,js,jsx,mjs,cjs}': ['oxlint --fix', 'prettier --write'],
  'site/**/*.{json,css,md,html,yml,yaml}': ['prettier --write'],
  '*.{mjs,cjs,js,json,md,yml,yaml}': ['prettier --write'],
  '**/*.rs': () => 'cargo fmt --all',
}
