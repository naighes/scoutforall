// the blocks of the DATA-PRIVACY file, the only source of the privacy page:
// the title, then paragraphs split by empty lines; a numbered line is a
// section, a short line with no punctuation a subsection, lines starting
// with "- " a list
import text from '../../../DATA-PRIVACY?raw';

const joined = (lines) => lines.map((l) => l.trim()).join(' ');

const block = (lines, index) => {
  const first = lines[0];
  switch (true) {
    case index === 0:
      return { kind: 'title', text: joined(lines) };
    case lines.length === 1 && /^\d+\. /.test(first):
      return { kind: 'section', text: first };
    case lines.length === 1 && !/[.:,]/.test(first):
      return { kind: 'subsection', text: first };
    case first.startsWith('- '):
      return {
        kind: 'list',
        items: lines
          .join('\n')
          .split(/^- /m)
          .filter((item) => item.trim() !== '')
          .map((item) => joined(item.split('\n'))),
      };
    default:
      return { kind: 'paragraph', text: joined(lines) };
  }
};

export const privacy = text
  .replace(/\r\n/g, '\n')
  .split(/\n\s*\n/)
  .map((b) => b.split('\n').filter((l) => l.trim() !== ''))
  .filter((lines) => lines.length > 0)
  .map(block);
