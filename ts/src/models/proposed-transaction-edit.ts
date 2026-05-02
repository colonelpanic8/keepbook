import { Id } from './id.js';
import {
  type TransactionAnnotationPatchType,
  type TransactionAnnotationPatchJSON,
} from './transaction-annotation.js';

export type ProposedTransactionEditStatus = 'pending' | 'approved' | 'rejected' | 'removed';

export interface ProposedTransactionEditType {
  readonly id: Id;
  readonly account_id: Id;
  readonly transaction_id: Id;
  readonly created_at: Date;
  readonly created_at_raw?: string;
  readonly updated_at: Date;
  readonly updated_at_raw?: string;
  readonly status: ProposedTransactionEditStatus;
  readonly description?: string | null;
  readonly note?: string | null;
  readonly category?: string | null;
  readonly subcategory?: string | null;
  readonly tags?: string[] | null;
  readonly effective_date?: string | null;
}

export interface ProposedTransactionEditJSON {
  id: string;
  account_id: string;
  transaction_id: string;
  created_at: string;
  updated_at: string;
  status: ProposedTransactionEditStatus;
  description?: string | null;
  note?: string | null;
  category?: string | null;
  subcategory?: string | null;
  tags?: string[] | null;
  effective_date?: string | null;
}

export const ProposedTransactionEdit = {
  toJSON(edit: ProposedTransactionEditType): ProposedTransactionEditJSON {
    const json: ProposedTransactionEditJSON = {
      id: edit.id.toJSON(),
      account_id: edit.account_id.toJSON(),
      transaction_id: edit.transaction_id.toJSON(),
      created_at: edit.created_at_raw ?? edit.created_at.toISOString(),
      updated_at: edit.updated_at_raw ?? edit.updated_at.toISOString(),
      status: edit.status,
    };
    if (edit.description !== undefined) json.description = edit.description;
    if (edit.note !== undefined) json.note = edit.note;
    if (edit.category !== undefined) json.category = edit.category;
    if (edit.subcategory !== undefined) json.subcategory = edit.subcategory;
    if (edit.tags !== undefined) json.tags = edit.tags;
    if (edit.effective_date !== undefined) json.effective_date = edit.effective_date;
    return json;
  },

  fromJSON(json: ProposedTransactionEditJSON): ProposedTransactionEditType {
    return {
      id: Id.fromString(json.id),
      account_id: Id.fromString(json.account_id),
      transaction_id: Id.fromString(json.transaction_id),
      created_at: new Date(json.created_at),
      created_at_raw: json.created_at,
      updated_at: new Date(json.updated_at),
      updated_at_raw: json.updated_at,
      status: json.status,
      description: json.description,
      note: json.note,
      category: json.category,
      subcategory: json.subcategory,
      tags: json.tags,
      effective_date: json.effective_date,
    };
  },

  withStatus(
    edit: ProposedTransactionEditType,
    status: ProposedTransactionEditStatus,
    now: Date,
  ): ProposedTransactionEditType {
    return {
      ...edit,
      status,
      updated_at: now,
      updated_at_raw: undefined,
    };
  },

  toAnnotationPatch(
    edit: ProposedTransactionEditType,
    timestamp: Date,
  ): TransactionAnnotationPatchType {
    return {
      transaction_id: edit.transaction_id,
      timestamp,
      ...(edit.description !== undefined ? { description: edit.description } : {}),
      ...(edit.note !== undefined ? { note: edit.note } : {}),
      ...(edit.category !== undefined ? { category: edit.category } : {}),
      ...(edit.subcategory !== undefined ? { subcategory: edit.subcategory } : {}),
      ...(edit.tags !== undefined ? { tags: edit.tags } : {}),
      ...(edit.effective_date !== undefined ? { effective_date: edit.effective_date } : {}),
    };
  },

  patchJSON(
    edit: ProposedTransactionEditType,
  ): Omit<TransactionAnnotationPatchJSON, 'transaction_id' | 'timestamp'> {
    return {
      ...(edit.description !== undefined ? { description: edit.description } : {}),
      ...(edit.note !== undefined ? { note: edit.note } : {}),
      ...(edit.category !== undefined ? { category: edit.category } : {}),
      ...(edit.subcategory !== undefined ? { subcategory: edit.subcategory } : {}),
      ...(edit.tags !== undefined ? { tags: edit.tags } : {}),
      ...(edit.effective_date !== undefined ? { effective_date: edit.effective_date } : {}),
    };
  },
} as const;
