/** src/api/schema.json から src/api/schema.gen.ts を作る。`aube run gen` で実行する。 */

import { readFileSync, writeFileSync } from 'node:fs';
import { generateValibotModule } from './json-schema-to-valibot.ts';

const apiDir = new URL('../src/api/', import.meta.url);
const schema: unknown = JSON.parse(readFileSync(new URL('schema.json', apiDir), 'utf8'));
if (typeof schema !== 'object' || schema === null || Array.isArray(schema)) {
  throw new Error('src/api/schema.json must contain a JSON object');
}
writeFileSync(
  new URL('schema.gen.ts', apiDir),
  generateValibotModule(schema as Record<string, unknown>),
);
