/**
 * Generast AST structures and generator utilities for Alhalte chat messages.
 *
 * References:
 * - ARCHITECTURE.md: `# 拡張機能 -> ## 拡張機能ABI`
 * - ARCHITECTURE.md: `## generast-rs`
 * - GitHub Issue #54: 共通フッターに関する提言
 */

import type { ExtensionItem } from '../types/index.js';
import { ExtensionPipeline } from '../wasm/runner.js';

export interface GenerastStringNode {
  type: 'string';
  value: string;
}

export interface GenerastElementNode {
  type: 'element';
  label: string;
  data?: unknown;
}

export interface GenerastNestedNode {
  type: 'nested';
  meta: string;
  contents: GenerastAST[];
}

export type GenerastAST = GenerastStringNode | GenerastElementNode | GenerastNestedNode;

/**
 * Creates a default plaintext Generast AST node.
 */
export function createTextGenerastAST(text: string): GenerastAST {
  return {
    type: 'string',
    value: text,
  };
}

/**
 * Generates a Generast AST from input text and optional characterId.
 *
 * If active extensions are present, executes the extension pipeline (or extension ABI runner)
 * to transform the Generast payload. Otherwise, returns a string node AST.
 */
export async function buildGenerastFromText(
  text: string,
  extensions: ExtensionItem[] = [],
  pipeline: ExtensionPipeline = new ExtensionPipeline(),
  characterId?: string,
): Promise<GenerastAST> {
  const activeExtensions = extensions.filter((ext) => ext.enabled);

  if (activeExtensions.length === 0) {
    return createTextGenerastAST(text);
  }

  try {
    // Initial Tin payload: plaintext encoded as bytes
    const initialBytes = new TextEncoder().encode(text);
    const resultBytes = await pipeline.executePipeline(activeExtensions, initialBytes);
    const decodedText = new TextDecoder().decode(resultBytes);

    return {
      type: 'string',
      value: decodedText,
    };
  } catch (err) {
    console.error('Failed to process message via extension pipeline:', err);
    // Fall back to plain text node AST on error
    return createTextGenerastAST(text);
  }
}
