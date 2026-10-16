import { PrismLight as SyntaxHighlighter } from 'react-syntax-highlighter'
import jsonLang from 'react-syntax-highlighter/dist/esm/languages/prism/json'
import markupLang from 'react-syntax-highlighter/dist/esm/languages/prism/markup'
import { tomorrow } from 'react-syntax-highlighter/dist/esm/styles/prism'
SyntaxHighlighter.registerLanguage('json', jsonLang)
SyntaxHighlighter.registerLanguage('markup', markupLang)
export function HL({ lang, code }: { lang: 'json' | 'markup'; code: string }) {
  return <SyntaxHighlighter language={lang} style={tomorrow}>{code}</SyntaxHighlighter>
}
