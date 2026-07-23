import { IconBrandGithub, IconCheck, IconCopy } from '@tabler/icons-react'
import { useCallback, useEffect, useState } from 'react'

async function copyText(text: string): Promise<boolean> {
  try {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text)
      return true
    }
  } catch {
    /* fall through */
  }

  try {
    const textarea = document.createElement('textarea')
    textarea.value = text
    textarea.setAttribute('readonly', '')
    textarea.style.position = 'fixed'
    textarea.style.left = '-9999px'
    document.body.appendChild(textarea)
    textarea.select()
    const copied = document.execCommand('copy')
    document.body.removeChild(textarea)
    return copied
  } catch {
    return false
  }
}

export function MarkdownActions({
  markdown,
  githubPath,
}: {
  markdown: string
  githubPath: string
}) {
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (!copied) return
    const id = window.setTimeout(() => setCopied(false), 1600)
    return () => window.clearTimeout(id)
  }, [copied])

  const onCopy = useCallback(async () => {
    if (await copyText(markdown)) setCopied(true)
  }, [markdown])

  return (
    <div className="markdown-actions" aria-label="Markdown actions">
      <button
        type="button"
        className="markdown-action"
        onClick={onCopy}
        aria-label={copied ? 'Markdown copied' : 'Copy Markdown'}
      >
        {copied ? (
          <IconCheck size={15} stroke={1.75} aria-hidden />
        ) : (
          <IconCopy size={15} stroke={1.75} aria-hidden />
        )}
        <span>{copied ? 'Copied' : 'Copy MD'}</span>
      </button>
      <a
        className="markdown-action"
        href={`https://github.com/auraspace/aura/blob/main/${githubPath}`}
        target="_blank"
        rel="noreferrer"
      >
        <IconBrandGithub size={15} stroke={1.75} aria-hidden />
        <span>GitHub</span>
      </a>
    </div>
  )
}
