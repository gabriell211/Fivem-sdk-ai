import type * as Monaco from 'monaco-editor';

const keywords = [
  'and', 'break', 'do', 'else', 'elseif', 'end', 'false', 'for', 'function',
  'goto', 'if', 'in', 'local', 'nil', 'not', 'or', 'repeat', 'return', 'then',
  'true', 'until', 'while',
];

const fivemFunctions = [
  'AddEventHandler',
  'CreateThread',
  'DeleteEntity',
  'DoesEntityExist',
  'GetCurrentResourceName',
  'GetEntityCoords',
  'GetEntityHeading',
  'GetEntityHealth',
  'GetEntityModel',
  'GetGameTimer',
  'GetHashKey',
  'GetPlayerServerId',
  'GetResourceState',
  'NetworkGetNetworkIdFromEntity',
  'PlayerId',
  'PlayerPedId',
  'RegisterCommand',
  'RegisterNetEvent',
  'RegisterNUICallback',
  'RequestModel',
  'SendNUIMessage',
  'SetEntityCoordsNoOffset',
  'SetEntityHeading',
  'SetHttpHandler',
  'SetNuiFocus',
  'TriggerClientEvent',
  'TriggerEvent',
  'TriggerServerEvent',
  'Wait',
];

export function configureMonaco(monaco: typeof Monaco): void {
  if (!monaco.languages.getLanguages().some((language) => language.id === 'lua')) {
    monaco.languages.register({
      id: 'lua',
      extensions: ['.lua'],
      aliases: ['Lua', 'lua'],
    });
  }

  monaco.languages.setMonarchTokensProvider('lua', {
    defaultToken: '',
    tokenPostfix: '.lua',
    keywords,
    brackets: [
      { open: '{', close: '}', token: 'delimiter.curly' },
      { open: '[', close: ']', token: 'delimiter.square' },
      { open: '(', close: ')', token: 'delimiter.parenthesis' },
    ],
    tokenizer: {
      root: [
        [/[a-zA-Z_][\w]*/, {
          cases: {
            '@keywords': 'keyword',
            '@default': 'identifier',
          },
        }],
        [/--\[\[/, 'comment', '@commentBlock'],
        [/--.*$/, 'comment'],
        [/[{}()[\]]/, '@brackets'],
        [/[<>]=?|==|~=|[-+*/%^#]/, 'operator'],
        [/0[xX][0-9a-fA-F]+/, 'number.hex'],
        [/\d+(\.\d+)?([eE][\-+]?\d+)?/, 'number'],
        [/"([^"\\]|\\.)*$/, 'string.invalid'],
        [/'([^'\\]|\\.)*$/, 'string.invalid'],
        [/"/, 'string', '@doubleString'],
        [/'/, 'string', '@singleString'],
      ],
      commentBlock: [
        [/\]\]/, 'comment', '@pop'],
        [/./, 'comment'],
      ],
      doubleString: [
        [/[^\\"]+/, 'string'],
        [/\\./, 'string.escape'],
        [/"/, 'string', '@pop'],
      ],
      singleString: [
        [/[^\\']+/, 'string'],
        [/\\./, 'string.escape'],
        [/'/, 'string', '@pop'],
      ],
    },
  });

  monaco.languages.registerCompletionItemProvider('lua', {
    triggerCharacters: [':', '.'],
    provideCompletionItems(model, position) {
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };

      const functions = fivemFunctions.map((label) => ({
        label,
        kind: monaco.languages.CompletionItemKind.Function,
        insertText: `${label}($0)`,
        insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
        range,
        detail: 'FiveM / Cfx runtime',
      }));

      const snippets = [
        {
          label: 'fivem:RegisterNetEvent',
          detail: 'Secure FiveM network event skeleton',
          insertText: "RegisterNetEvent('${1:event}', function(${2:payload})\n    ${0}\nend)",
        },
        {
          label: 'fivem:RegisterNUICallback',
          detail: 'NUI callback that always responds',
          insertText: "RegisterNUICallback('${1:action}', function(data, cb)\n    ${2}\n    cb({ ok = true })\nend)",
        },
        {
          label: 'fivem:CreateThread',
          detail: 'Citizen thread skeleton',
          insertText: "CreateThread(function()\n    while true do\n        Wait(${1:1000})\n        ${0}\n    end\nend)",
        },
        {
          label: 'fivem:fxmanifest',
          detail: 'Current FiveM resource manifest baseline',
          insertText: "fx_version 'cerulean'\ngame 'gta5'\n\nauthor '${1:author}'\ndescription '${2:resource}'\nversion '1.0.0'\n\nclient_scripts {\n    '${3:client.lua}'\n}\n\nserver_scripts {\n    '${4:server.lua}'\n}",
        },
      ].map((item) => ({
        ...item,
        kind: monaco.languages.CompletionItemKind.Snippet,
        insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
        range,
      }));

      return { suggestions: [...snippets, ...functions] };
    },
  });
}
