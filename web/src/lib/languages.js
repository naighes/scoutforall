// the languages of the site: english is the home page, the others live
// under their code (/scoutforall/<code>/), each with its own data folder
export const languages = [
  { code: 'en', label: 'English', hreflang: 'en', locale: 'en_US' },
  { code: 'it', label: 'Italiano', hreflang: 'it', locale: 'it_IT' },
  { code: 'pt', label: 'Português (Brasil)', hreflang: 'pt-BR', locale: 'pt_BR' },
  { code: 'pl', label: 'Polski', hreflang: 'pl', locale: 'pl_PL' },
  { code: 'ja', label: '日本語', hreflang: 'ja', locale: 'ja_JP' },
];

export const language = (code) =>
  languages.find((l) => l.code === code) ?? languages[0];

// the address of the home page in a language
export const homeUrl = (code) =>
  `https://naighes.github.io/scoutforall/${code === 'en' ? '' : code + '/'}`;

const data = import.meta.glob('../data/*/*.json', { eager: true, import: 'default' });

// a data file (faq, home, ...) in a language
export const content = (code, name) => data[`../data/${language(code).code}/${name}.json`];
