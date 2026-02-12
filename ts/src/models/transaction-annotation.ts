import { Id } from './id.js';

// ---------------------------------------------------------------------------
// Materialized state (current view)
// ---------------------------------------------------------------------------

export interface TransactionAnnotationType {
  readonly transaction_id: Id;
  readonly description?: string;
  readonly note?: string;
  readonly category?: string;
  readonly tags?: string[];
}

export function isEmptyTransactionAnnotation(a: TransactionAnnotationType): boolean {
  return (
    a.description === undefined &&
    a.note === undefined &&
    a.category === undefined &&
    a.tags === undefined
  );
}

// ---------------------------------------------------------------------------
// Patch types (append-only, tri-state fields)
// ---------------------------------------------------------------------------

export interface TransactionAnnotationPatchType {
  readonly transaction_id: Id;
  readonly timestamp: Date;
  readonly timestamp_raw?: string;

  // Field semantics:
  // - undefined: no change
  // - null: clear
  // - value: set/overwrite
  readonly description?: string | null;
  readonly note?: string | null;
  readonly category?: string | null;
  readonly tags?: string[] | null;
}

export interface TransactionAnnotationPatchJSON {
  transaction_id: string;
  timestamp: string;
  description?: string | null;
  note?: string | null;
  category?: string | null;
  tags?: string[] | null;
}

export const TransactionAnnotationPatch = {
  toJSON(p: TransactionAnnotationPatchType): TransactionAnnotationPatchJSON {
    const json: TransactionAnnotationPatchJSON = {
      transaction_id: p.transaction_id.toJSON(),
      timestamp: p.timestamp_raw ?? p.timestamp.toISOString(),
    };
    if (p.description !== undefined) json.description = p.description;
    if (p.note !== undefined) json.note = p.note;
    if (p.category !== undefined) json.category = p.category;
    if (p.tags !== undefined) json.tags = p.tags;
    return json;
  },

  fromJSON(json: TransactionAnnotationPatchJSON): TransactionAnnotationPatchType {
    return {
      transaction_id: Id.fromString(json.transaction_id),
      timestamp: new Date(json.timestamp),
      timestamp_raw: json.timestamp,
      description: json.description,
      note: json.note,
      category: json.category,
      tags: json.tags,
    };
  },
} as const;

// ---------------------------------------------------------------------------
// Patch application
// ---------------------------------------------------------------------------

export function applyTransactionAnnotationPatch(
  base: TransactionAnnotationType,
  patch: TransactionAnnotationPatchType,
): TransactionAnnotationType {
  let out: TransactionAnnotationType = base;

  if (patch.description !== undefined) {
    const { description: _old, ...rest } = out as TransactionAnnotationType & { description?: string };
    out =
      patch.description === null
        ? rest
        : { ...rest, description: patch.description };
  }
  if (patch.note !== undefined) {
    const { note: _old, ...rest } = out as TransactionAnnotationType & { note?: string };
    out = patch.note === null ? rest : { ...rest, note: patch.note };
  }
  if (patch.category !== undefined) {
    const { category: _old, ...rest } = out as TransactionAnnotationType & { category?: string };
    out = patch.category === null ? rest : { ...rest, category: patch.category };
  }
  if (patch.tags !== undefined) {
    const { tags: _old, ...rest } = out as TransactionAnnotationType & { tags?: string[] };
    out = patch.tags === null ? rest : { ...rest, tags: patch.tags };
  }

  return out;
}

