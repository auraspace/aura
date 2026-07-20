// Load Vite’s ambient client types by path (not `"types": ["vite/client"]`).
// The compilerOptions `types` entry fails under pnpm:
// "Cannot find type definition file for 'vite/client'".
/// <reference path="../node_modules/vite/client.d.ts" />

declare module '*.md?raw' {
  const content: string
  export default content
}
