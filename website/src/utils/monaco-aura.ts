import { Monaco } from '@monaco-editor/react';

export const registerAuraLanguage = (monaco: Monaco) => {
  // Register a new language
  monaco.languages.register({ id: 'aura' });

  // Register a tokens provider for the language
  monaco.languages.setMonarchTokensProvider('aura', {
    defaultToken: 'invalid',
    tokenPostfix: '.aura',

    keywords: [
      'if', 'else', 'while', 'for', 'return', 'try', 'catch', 'throw', 'finally',
      'match', 'new', 'import', 'from', 'export', 'as', 'async', 'await', 'is',
      'implements', 'extends', 'class', 'constructor', 'interface', 'type',
      'enum', 'abstract', 'override', 'static', 'readonly', 'public',
      'protected', 'private', 'let', 'const', 'var', 'this', 'print', 'super',
      'function', 'mut', 'break', 'continue', 'nil', 'println', 'loop', 'in'
    ],

    typeKeywords: [
      'i32', 'i64', 'u32', 'u64', 'f32', 'f64', 'bool', 'string', 'char', 'void',
      'number', 'any', 'unknown', 'Promise', 'TCPStream', 'TCPServer',
      'HTTPClient', 'HTTPRequest', 'HTTPResponse', 'Date', 'Error', 'Result',
      'Map', 'Set', 'Array'
    ],

    operators: [
      '=', '>', '<', '!', '~', '?', ':', '==', '<=', '>=', '!=', '&&', '||',
      '++', '--', '+', '-', '*', '/', '&', '|', '^', '%', '<<', '>>', '>>>',
      '+=', '-=', '*=', '/=', '&=', '|=', '^=', '%=', '<<=', '>>=', '>>>=',
      '??', '|>', '=>'
    ],

    // we include these common regular expressions
    symbols: /[=><!~?:&|+\-*\/\^%]+/,

    // C# style strings
    escapes: /\\(?:[abfnrtv\\"']|x[0-9A-Fa-f]{1,4}|u[0-9A-Fa-f]{4}|U[0-9A-Fa-f]{8})/,

    tokenizer: {
      root: [
        // identifiers and keywords
        [/[a-z_$][\w$]*/, {
          cases: {
            '@typeKeywords': 'keyword',
            '@keywords': 'keyword',
            '@default': 'identifier'
          }
        }],

        [/[A-Z][\w\$]*/, 'type.identifier'],  // to show class names nicely

        // whitespace
        { include: '@whitespace' },

        // delimiters and operators
        [/[{}()\[\]]/, '@brackets'],
        [/[<>](?!@symbols)/, '@brackets'],
        [/@symbols/, {
          cases: {
            '@operators': 'operator',
            '@default': ''
          }
        }],

        // @ annotations
        [/@[a-zA-Z_]\w*/, 'tag'],

        // numbers
        [/\d*\.\d+([eE][\-+]?\d+)?/, 'number.float'],
        [/0[xX][0-9a-fA-F]+/, 'number.hex'],
        [/\d+/, 'number'],

        // delimiter: after number because of .\d floats
        [/[;,.]/, 'delimiter'],

        // strings
        [/"([^"\\]|\\.)*$/, 'string.invalid'],  // non-teminated string
        [/"/, { token: 'string.quote', bracket: '@open', next: '@string' }],

        // template literals
        [/`/, { token: 'string.quote', bracket: '@open', next: '@string_backtick' }],

        // characters
        [/'[^\\']'/, 'string'],
        [/(')(@escapes)(')/, ['string', 'string.escape', 'string']],
        [/'/, 'string.invalid']
      ],

      comment: [
        [/[^\/*]+/, 'comment'],
        [/\/\*/, 'comment', '@push'],    // nested comment
        ["\\*/", 'comment', '@pop'],
        [/[\/*]/, 'comment']
      ],

      string: [
        [/[^\\"]+/, 'string'],
        [/@escapes/, 'string.escape'],
        [/\\./, 'string.escape.invalid'],
        [/"/, { token: 'string.quote', bracket: '@close', next: '@pop' }]
      ],

      string_backtick: [
        [/\$\{/, { token: 'delimiter.bracket', next: '@bracket_content' }],
        [/[^\\`$]+/, 'string'],
        [/@escapes/, 'string.escape'],
        [/\\./, 'string.escape.invalid'],
        [/`/, { token: 'string.quote', bracket: '@close', next: '@pop' }]
      ],

      bracket_content: [
        [/\}/, { token: 'delimiter.bracket', next: '@pop' }],
        { include: 'root' }
      ],

      whitespace: [
        [/[ \t\r\n]+/, 'white'],
        [/\/\*/, 'comment', '@comment'],
        [/\/\/\/.*$/, 'comment.doc'],
        [/\/\/.*$/, 'comment'],
      ],
    },
  });

  // Language configuration
  monaco.languages.setLanguageConfiguration('aura', {
    comments: {
      lineComment: '//',
      blockComment: ['/*', '*/'],
    },
    brackets: [
      ['{', '}'],
      ['[', ']'],
      ['(', ')'],
    ],
    autoClosingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
      { open: '`', close: '`' },
    ],
    surroundingPairs: [
      { open: '{', close: '}' },
      { open: '[', close: ']' },
      { open: '(', close: ')' },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
      { open: '`', close: '`' },
    ],
  });
};
