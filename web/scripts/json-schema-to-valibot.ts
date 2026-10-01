/**
 * schemars が出す JSON Schema (draft 2020-12) を、valibot のスキーマの TypeScript コードに変換する。
 *
 * 対応するのは kgd のビューアの API が使う範囲に限る。知らないキーワードに出会ったら、
 * 黙って無視せずに UnsupportedSchemaError を投げる。
 */

type JsonObject = Record<string, unknown>;

/** 対応していない JSON Schema に出会ったことを表す。 */
export class UnsupportedSchemaError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'UnsupportedSchemaError';
  }
}

/** 検証に影響しない注釈のキーワード。 */
const ANNOTATIONS = ['$schema', 'title', 'description'];

/** 変換できるキーワード。 */
const SUPPORTED_KEYS = new Set([
  ...ANNOTATIONS,
  '$ref',
  'anyOf',
  'oneOf',
  'enum',
  'const',
  'type',
  'properties',
  'required',
  'additionalProperties',
  'items',
  'prefixItems',
  'minItems',
  'maxItems',
  'format',
  'minimum',
]);

/** schemars が Rust の数値型から付ける format。値の検証には影響しない。 */
const NUMERIC_FORMATS = new Set([
  'double',
  'float',
  'int',
  'int8',
  'int16',
  'int32',
  'int64',
  'uint',
  'uint8',
  'uint16',
  'uint32',
  'uint64',
]);

/** `$defs` の名前として受け付ける形。TypeScript の識別子になる。 */
const DEFINITION_NAME = /^[A-Za-z_][A-Za-z0-9_]*$/;

/**
 * ルートのスキーマから、`$defs` の各型とルートの valibot スキーマを並べたモジュールを作る。
 *
 * ルートの名前は `title` から取る。参照される型が先に来るように並べる。
 */
export function generateValibotModule(root: JsonObject): string {
  const { $defs, ...rootSchema } = root;
  const rootName = root.title;
  if (typeof rootName !== 'string' || !DEFINITION_NAME.test(rootName)) {
    throw new UnsupportedSchemaError('root: "title" must be a valid identifier');
  }
  const definitions: Record<string, JsonObject> = {
    ...(asObject($defs ?? {}, '#/$defs') as Record<string, JsonObject>),
    [rootName]: rootSchema,
  };

  const lines = [
    '// scripts/gen-api.ts が src/api/schema.json から生成する。手で編集しないこと。',
    "import * as v from 'valibot';",
    '',
  ];
  for (const name of dependencyOrder(definitions)) {
    lines.push(`export const ${name}Schema = ${convertSchema(definitions[name], name)};`);
    lines.push(`export type ${name} = v.InferOutput<typeof ${name}Schema>;`);
    lines.push('');
  }
  return lines.join('\n');
}

/** 1 つのスキーマを valibot の式に変換する。`path` はエラーの場所の表示に使う。 */
export function convertSchema(schema: unknown, path: string): string {
  const s = asObject(schema, path);
  for (const key of Object.keys(s)) {
    if (!SUPPORTED_KEYS.has(key)) {
      throw new UnsupportedSchemaError(`${path}: unsupported keyword "${key}"`);
    }
  }

  if (s.$ref !== undefined) {
    return `${refName(s.$ref, path)}Schema`;
  }
  if (s.anyOf !== undefined) {
    return convertNullableUnion(s.anyOf, path);
  }
  if (s.oneOf !== undefined) {
    return convertConstUnion(s.oneOf, path);
  }
  if (s.enum !== undefined) {
    return picklist(s.enum, path);
  }
  if (s.const !== undefined) {
    return literal(s.const, path);
  }
  if (Array.isArray(s.type)) {
    const others = s.type.filter((type) => type !== 'null');
    if (others.length !== 1 || others.length === s.type.length) {
      throw new UnsupportedSchemaError(`${path}: only "[X, null]" type unions are supported`);
    }
    return `v.nullable(${convertSchema({ ...s, type: others[0] }, path)})`;
  }

  switch (s.type) {
    case 'object':
      return convertObject(s, path);
    case 'array':
      return convertArray(s, path);
    case 'string':
      return convertString(s, path);
    case 'number':
      return convertNumber(s, path, false);
    case 'integer':
      return convertNumber(s, path, true);
    case 'boolean':
      return 'v.boolean()';
    case 'null':
      return 'v.null()';
    default:
      throw new UnsupportedSchemaError(`${path}: unsupported type ${JSON.stringify(s.type)}`);
  }
}

function convertObject(s: JsonObject, path: string): string {
  const properties = asObject(s.properties ?? {}, `${path}.properties`);
  const required = new Set(stringArray(s.required ?? [], `${path}.required`));
  if (s.additionalProperties !== undefined && typeof s.additionalProperties !== 'boolean') {
    throw new UnsupportedSchemaError(`${path}: only boolean "additionalProperties" is supported`);
  }
  const entries = Object.entries(properties).map(([key, value]) => {
    const inner = convertSchema(value, `${path}.${key}`);
    return `${JSON.stringify(key)}: ${required.has(key) ? inner : `v.optional(${inner})`}`;
  });
  const factory = s.additionalProperties === false ? 'v.strictObject' : 'v.object';
  return `${factory}({ ${entries.join(', ')} })`;
}

function convertArray(s: JsonObject, path: string): string {
  if (s.prefixItems !== undefined) {
    if (s.items !== undefined && s.items !== false) {
      throw new UnsupportedSchemaError(`${path}: "prefixItems" with extra "items" is unsupported`);
    }
    const items = arrayOf(s.prefixItems, `${path}.prefixItems`).map((item, index) =>
      convertSchema(item, `${path}[${index}]`),
    );
    return `v.tuple([${items.join(', ')}])`;
  }
  const actions: string[] = [];
  if (s.minItems !== undefined) {
    actions.push(`v.minLength(${nonNegativeInteger(s.minItems, `${path}.minItems`)})`);
  }
  if (s.maxItems !== undefined) {
    actions.push(`v.maxLength(${nonNegativeInteger(s.maxItems, `${path}.maxItems`)})`);
  }
  return pipe(`v.array(${convertSchema(s.items, `${path}[]`)})`, actions);
}

function convertString(s: JsonObject, path: string): string {
  switch (s.format) {
    case undefined:
      return 'v.string()';
    case 'date':
      return 'v.pipe(v.string(), v.isoDate())';
    case 'date-time':
      return 'v.pipe(v.string(), v.isoTimestamp())';
    default:
      throw new UnsupportedSchemaError(
        `${path}: unsupported string format ${JSON.stringify(s.format)}`,
      );
  }
}

function convertNumber(s: JsonObject, path: string, integer: boolean): string {
  if (s.format !== undefined && !NUMERIC_FORMATS.has(String(s.format))) {
    throw new UnsupportedSchemaError(
      `${path}: unsupported number format ${JSON.stringify(s.format)}`,
    );
  }
  const actions: string[] = [];
  if (integer) {
    actions.push('v.integer()');
  }
  if (s.minimum !== undefined) {
    if (typeof s.minimum !== 'number') {
      throw new UnsupportedSchemaError(`${path}: "minimum" must be a number`);
    }
    actions.push(`v.minValue(${s.minimum})`);
  }
  return pipe('v.number()', actions);
}

function convertNullableUnion(members: unknown, path: string): string {
  const list = arrayOf(members, `${path}.anyOf`);
  const isNull = (member: unknown) => {
    const object = asObject(member, `${path}.anyOf`);
    return (
      object.type === 'null' &&
      Object.keys(object).every((key) => key === 'type' || ANNOTATIONS.includes(key))
    );
  };
  const others = list.filter((member) => !isNull(member));
  if (list.length !== 2 || others.length !== 1) {
    throw new UnsupportedSchemaError(`${path}: only "anyOf: [X, null]" is supported`);
  }
  return `v.nullable(${convertSchema(others[0], `${path}.anyOf`)})`;
}

function convertConstUnion(members: unknown, path: string): string {
  const values = arrayOf(members, `${path}.oneOf`).map((member, index) => {
    const object = asObject(member, `${path}.oneOf[${index}]`);
    const extra = Object.keys(object).filter(
      (key) => key !== 'const' && key !== 'type' && !ANNOTATIONS.includes(key),
    );
    if (typeof object.const !== 'string' || extra.length > 0) {
      throw new UnsupportedSchemaError(`${path}: only "oneOf" of string constants is supported`);
    }
    return object.const;
  });
  return `v.picklist(${JSON.stringify(values)})`;
}

function picklist(values: unknown, path: string): string {
  return `v.picklist(${JSON.stringify(stringArray(values, `${path}.enum`))})`;
}

function literal(value: unknown, path: string): string {
  if (typeof value !== 'string' && typeof value !== 'number' && typeof value !== 'boolean') {
    throw new UnsupportedSchemaError(`${path}: "const" must be a string, number or boolean`);
  }
  return `v.literal(${JSON.stringify(value)})`;
}

/** `$ref` から `$defs` の名前を取り出す。 */
function refName(ref: unknown, path: string): string {
  const match = typeof ref === 'string' ? /^#\/\$defs\/(.+)$/.exec(ref) : null;
  const name = match?.[1];
  if (name === undefined || !DEFINITION_NAME.test(name)) {
    throw new UnsupportedSchemaError(`${path}: unsupported $ref ${JSON.stringify(ref)}`);
  }
  return name;
}

/** 参照される定義が先に来る順序で名前を返す。循環と未知の参照は拒否する。 */
function dependencyOrder(definitions: Record<string, JsonObject>): string[] {
  const order: string[] = [];
  const state = new Map<string, 'visiting' | 'done'>();
  const visit = (name: string, from: string) => {
    const definition = definitions[name];
    if (definition === undefined) {
      throw new UnsupportedSchemaError(`${from}: unknown definition "${name}"`);
    }
    if (state.get(name) === 'done') {
      return;
    }
    if (state.get(name) === 'visiting') {
      throw new UnsupportedSchemaError(`${from}: circular reference to "${name}"`);
    }
    state.set(name, 'visiting');
    for (const ref of collectRefs(definition, name)) {
      visit(ref, name);
    }
    state.set(name, 'done');
    order.push(name);
  };
  for (const name of Object.keys(definitions)) {
    visit(name, name);
  }
  return order;
}

/** スキーマの中の `$ref` をすべて集める。 */
function collectRefs(value: unknown, path: string): string[] {
  if (Array.isArray(value)) {
    return value.flatMap((item) => collectRefs(item, path));
  }
  if (typeof value !== 'object' || value === null) {
    return [];
  }
  return Object.entries(value).flatMap(([key, inner]) =>
    key === '$ref' ? [refName(inner, path)] : collectRefs(inner, path),
  );
}

function pipe(base: string, actions: string[]): string {
  return actions.length === 0 ? base : `v.pipe(${[base, ...actions].join(', ')})`;
}

function asObject(value: unknown, path: string): JsonObject {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new UnsupportedSchemaError(`${path}: expected an object`);
  }
  return value as JsonObject;
}

function arrayOf(value: unknown, path: string): unknown[] {
  if (!Array.isArray(value)) {
    throw new UnsupportedSchemaError(`${path}: expected an array`);
  }
  return value;
}

function stringArray(value: unknown, path: string): string[] {
  const list = arrayOf(value, path);
  if (!list.every((item) => typeof item === 'string')) {
    throw new UnsupportedSchemaError(`${path}: expected an array of strings`);
  }
  return list as string[];
}

function nonNegativeInteger(value: unknown, path: string): number {
  if (typeof value !== 'number' || !Number.isInteger(value) || value < 0) {
    throw new UnsupportedSchemaError(`${path}: expected a non-negative integer`);
  }
  return value;
}
