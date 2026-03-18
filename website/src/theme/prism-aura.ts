import { Prism } from 'prism-react-renderer';

(Prism.languages as any).aura = {
  comment: [
    {
      pattern: /\/\*\*[\s\S]*?\*\//,
      greedy: true,
      alias: 'doc-comment',
    },
    {
      pattern: /\/\*[\s\S]*?\*\//,
      greedy: true,
    },
    {
      pattern: /\/\/\/.*$/m,
      alias: 'doc-comment',
    },
    {
      pattern: /\/\/.*$/m,
    },
  ],
  string: [
    {
      pattern: /"(?:\\.|[^"\\\r\n])*"/,
      greedy: true,
    },
    {
      pattern: /'(?:\\.|[^'\\\r\n])*'/,
      greedy: true,
    },
    {
      pattern: /`[\s\S]*?`/,
      greedy: true,
      inside: {
        interpolation: {
          pattern: /\$\{[\s\S]+?\}/,
          inside: {
            punctuation: /^\$\{Reference\}|\}$/,
            expression: {
              pattern: /[\s\S]+/,
              inside: null as any,
            },
          },
        },
        string: /[\s\S]+/,
      },
    },
  ],
  decorator: {
    pattern: /@[a-zA-Z_]\w*/,
    alias: 'atrule',
  },
  keyword:
    /\b(?:if|else|while|for|return|try|catch|throw|finally|match|new|import|from|export|as|async|await|is|implements|extends|class|constructor|interface|type|enum|abstract|override|static|readonly|public|protected|private|let|const|var|this|print|super|function|mut|break|continue|nil|println|loop|in)\b/,
  boolean: /\b(?:true|false)\b/,
  'constant-language': /\b(?:null|nil)\b/,
  'builtin-type':
    /\b(?:i32|i64|u32|u64|f32|f64|bool|string|char|void|number|any|unknown|Promise|TCPStream|TCPServer|HTTPClient|HTTPRequest|HTTPResponse|Date|Error|Result|Map|Set|Array)\b/,
  'class-name': /\b[A-Z]\w*\b/,
  function: /\b\w+(?=\s*\()/,
  number: /\b\d+(?:\.\d+)?(?:[eE][+-]?\d+)?\b/,
  operator:
    /\*\*|==|!=|<=|>=|&&|\|\||<<|>>|\+=|-=|\*=|\/=|%=|\?\?|\|>|=>|[+*/%<>!=^|&~?.=-]/,
  punctuation: /[{}[\];(),.]/,
};

// Circular reference for template literals
const aura = (Prism.languages as any).aura;
if (aura.string[2].inside.interpolation.inside.expression) {
  aura.string[2].inside.interpolation.inside.expression.inside = aura;
}
