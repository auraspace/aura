import { renderToString } from 'react-dom/server'
import { StaticRouter } from 'react-router'
import { App } from './app'

export function render(url: string, basename?: string) {
  const html = renderToString(
    <StaticRouter location={url} basename={basename}>
      <App />
    </StaticRouter>,
  )
  return html
}
