/**
 * bit-lang — The .bit language toolkit
 * @packageDocumentation
 */

/** A parsed .bit AST node */
export interface BitNode {
  kind: string;
  [key: string]: unknown;
}

/** A parsed .bit document */
export interface BitDocument {
  nodes: BitNode[];
}

/** Parse error from .bit source */
export interface BitParseError {
  message: string;
  line?: number;
  column?: number;
}

/** Validation result */
export interface BitValidationResult {
  ok: boolean;
  errors: BitParseError[];
}

/** Document index with task/entity counts */
export interface BitDocIndex {
  groups: unknown[];
  tasks: unknown[];
  refs: unknown[];
  flows: unknown[];
  [key: string]: unknown;
}

/**
 * Parse .bit source text into an AST
 * @param source - .bit source text
 * @returns Parsed document AST
 * @throws {Error} If source contains parse errors
 */
export function parse(source: string): BitDocument;

/**
 * Compile .bit source to IR (intermediate representation)
 * @param source - .bit source text
 * @returns Compiled IR
 */
export function compile(source: string): unknown;

/**
 * Format .bit source text with consistent style
 * @param source - .bit source text
 * @returns Formatted .bit text
 */
export function fmt(source: string): string;

/**
 * Render a Document AST (as JSON string) back to .bit text
 * @param docJson - JSON string of a BitDocument
 * @returns .bit formatted text
 */
export function render(docJson: string): string;

/**
 * Convert JSON data to .bit format
 * @param json - JSON string
 * @returns .bit formatted text
 */
export function fromJson(json: string): string;

/**
 * Convert Markdown to .bit format
 * @param md - Markdown text
 * @returns .bit formatted text
 */
export function fromMarkdown(md: string): string;

/**
 * Convert .bit source to JSON
 * @param source - .bit source text
 * @returns JSON string
 */
export function toJson(source: string): string;

/**
 * Validate .bit source against a schema
 * @param source - .bit source text to validate
 * @param schemaSource - .bit schema source text
 * @returns Validation result
 */
export function validate(source: string, schemaSource: string): BitValidationResult;

/**
 * Build a document index from .bit source
 * @param source - .bit source text
 * @returns Document index with task/entity counts
 */
export function buildIndex(source: string): BitDocIndex;
