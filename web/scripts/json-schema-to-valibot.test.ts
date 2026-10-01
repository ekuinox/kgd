import { describe, expect, it } from 'vitest';
import {
  convertSchema,
  generateValibotModule,
  UnsupportedSchemaError,
} from './json-schema-to-valibot.ts';

describe('convertSchema', () => {
  it('converts primitive types and ignores numeric formats', () => {
    expect(convertSchema({ type: 'string' }, 'x')).toBe('v.string()');
    expect(convertSchema({ type: 'number', format: 'double' }, 'x')).toBe('v.number()');
    expect(convertSchema({ type: 'integer', format: 'uint', minimum: 0 }, 'x')).toBe(
      'v.pipe(v.number(), v.integer(), v.minValue(0))',
    );
    expect(convertSchema({ type: 'boolean', description: 'flag' }, 'x')).toBe('v.boolean()');
  });

  it('converts date and date-time strings', () => {
    expect(convertSchema({ type: 'string', format: 'date' }, 'x')).toBe(
      'v.pipe(v.string(), v.isoDate())',
    );
    expect(convertSchema({ type: 'string', format: 'date-time' }, 'x')).toBe(
      'v.pipe(v.string(), v.isoTimestamp())',
    );
  });

  it('converts nullable types in both forms', () => {
    expect(convertSchema({ type: ['string', 'null'], format: 'date-time' }, 'x')).toBe(
      'v.nullable(v.pipe(v.string(), v.isoTimestamp()))',
    );
    expect(convertSchema({ anyOf: [{ $ref: '#/$defs/Kind' }, { type: 'null' }] }, 'x')).toBe(
      'v.nullable(KindSchema)',
    );
  });

  it('converts string enums in both forms', () => {
    expect(
      convertSchema(
        {
          oneOf: [
            { type: 'string', const: 'walking', description: '徒歩' },
            { type: 'string', const: 'automotive', description: '車' },
          ],
        },
        'x',
      ),
    ).toBe('v.picklist(["walking","automotive"])');
    expect(convertSchema({ type: 'string', enum: ['Feature'] }, 'x')).toBe(
      'v.picklist(["Feature"])',
    );
    expect(convertSchema({ const: 'Feature' }, 'x')).toBe('v.literal("Feature")');
  });

  it('converts objects with required and optional properties', () => {
    const schema = {
      type: 'object',
      properties: { from: { type: 'string' }, note: { type: ['string', 'null'] } },
      required: ['from'],
    };
    expect(convertSchema(schema, 'x')).toBe(
      'v.object({ "from": v.string(), "note": v.optional(v.nullable(v.string())) })',
    );
    expect(convertSchema({ ...schema, additionalProperties: false }, 'x')).toBe(
      'v.strictObject({ "from": v.string(), "note": v.optional(v.nullable(v.string())) })',
    );
  });

  it('converts arrays and tuples', () => {
    expect(
      convertSchema(
        { type: 'array', items: { type: 'number', format: 'double' }, minItems: 2, maxItems: 2 },
        'x',
      ),
    ).toBe('v.pipe(v.array(v.number()), v.minLength(2), v.maxLength(2))');
    expect(
      convertSchema(
        {
          type: 'array',
          prefixItems: [{ type: 'number' }, { type: 'number' }],
          items: false,
          minItems: 2,
          maxItems: 2,
        },
        'x',
      ),
    ).toBe('v.tuple([v.number(), v.number()])');
  });

  it('rejects unsupported keywords and formats', () => {
    expect(() => convertSchema({ type: 'string', pattern: '^a' }, 'x.name')).toThrow(
      UnsupportedSchemaError,
    );
    expect(() => convertSchema({ type: 'string', format: 'email' }, 'x')).toThrow(
      UnsupportedSchemaError,
    );
    expect(() => convertSchema({ allOf: [] }, 'x')).toThrow(UnsupportedSchemaError);
    expect(() => convertSchema({ $ref: 'other.json#/X' }, 'x')).toThrow(UnsupportedSchemaError);
  });
});

describe('generateValibotModule', () => {
  it('emits definitions after the definitions they reference', () => {
    const code = generateValibotModule({
      $schema: 'https://json-schema.org/draft/2020-12/schema',
      title: 'Root',
      type: 'object',
      properties: { a: { $ref: '#/$defs/A' } },
      required: ['a'],
      $defs: {
        A: {
          type: 'object',
          properties: { b: { $ref: '#/$defs/B' } },
          required: ['b'],
        },
        B: { type: 'string' },
      },
    });

    const b = code.indexOf('export const BSchema');
    const a = code.indexOf('export const ASchema');
    const root = code.indexOf('export const RootSchema');
    expect(b).toBeGreaterThan(-1);
    expect(b).toBeLessThan(a);
    expect(a).toBeLessThan(root);
    expect(code).toContain("import * as v from 'valibot';");
    expect(code).toContain('export type A = v.InferOutput<typeof ASchema>;');
  });

  it('rejects circular references and unknown definitions', () => {
    expect(() =>
      generateValibotModule({
        title: 'Root',
        $ref: '#/$defs/A',
        $defs: { A: { $ref: '#/$defs/A' } },
      }),
    ).toThrow(UnsupportedSchemaError);
    expect(() => generateValibotModule({ title: 'Root', $ref: '#/$defs/Missing' })).toThrow(
      UnsupportedSchemaError,
    );
  });
});
