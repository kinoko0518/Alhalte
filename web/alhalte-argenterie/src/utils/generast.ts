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

export type GenerastAST =
  | GenerastStringNode
  | GenerastElementNode
  | GenerastNestedNode
  | Uint8Array;

/**
 * 64-bit label hash for `alhalte/generast`:
 * `xxh64("alhalte/generast", 0) = 0xd38f9ce233c2315an`
 */
export const LABEL_ALHALTE_GENERAST = 0xd38f9ce233c2315an;

/**
 * Encodes a 64-bit unsigned integer or number into Postcard varint bytes.
 */
export function encodeVarint(value: bigint | number): number[] {
  let v = BigInt.asUintN(64, BigInt(value));
  const out: number[] = [];
  while (v >= 0x80n) {
    out.push(Number((v & 0x7fn) | 0x80n));
    v >>= 7n;
  }
  out.push(Number(v));
  return out;
}

/**
 * Encodes a plaintext string and speaker_id into a Tin binary formatted as `(Generast::String, u64)` for extension ABI I/O.
 *
 * Wire layout:
 * - 8 bytes: Tin label `LABEL_ALHALTE_GENERAST` (u64 little-endian, `0xd38f9ce233c2315an`)
 * - Postcard payload `(Generast::String(text), speaker_id: u64)`:
 *   - Variant 1 (Generast::String): 1 byte `1`
 *   - Varint string length (byte count)
 *   - UTF-8 string bytes
 *   - Varint speaker_id (u64)
 */
export function encodeExtIoTin(text: string, speakerId: number | bigint = 0): Uint8Array {
  const textBytes = new TextEncoder().encode(text);
  const payload = [
    1, // Variant 1: Generast::String
    ...encodeVarint(textBytes.length),
    ...textBytes,
    ...encodeVarint(speakerId),
  ];

  const tinBytes = new Uint8Array(8 + payload.length);
  const dv = new DataView(tinBytes.buffer);
  dv.setBigUint64(0, LABEL_ALHALTE_GENERAST, true);
  tinBytes.set(payload, 8);
  return tinBytes;
}

/**
 * Decodes a plaintext string and speaker_id from a Tin binary formatted as `(Generast::String, u64)`.
 */
export function decodeExtIoTin(bytes: Uint8Array): { text: string; speakerId: bigint } {
  if (bytes.length < 8) {
    throw new Error('Tin bytes too short');
  }
  const dv = new DataView(bytes.buffer, bytes.byteOffset, 8);
  const label = dv.getBigUint64(0, true);
  if (label !== LABEL_ALHALTE_GENERAST) {
    throw new Error(`Label mismatch: expected ${LABEL_ALHALTE_GENERAST}, got ${label}`);
  }
  const payload = bytes.subarray(8);
  if (payload.length === 0 || payload[0] !== 1) {
    throw new Error('Expected Generast::String variant 1');
  }
  let offset = 1;
  let len = 0;
  let shift = 0;
  while (offset < payload.length) {
    const b = payload[offset++];
    len |= (b & 0x7f) << shift;
    if ((b & 0x80) === 0) break;
    shift += 7;
  }
  const textBytes = payload.subarray(offset, offset + len);
  const text = new TextDecoder().decode(textBytes);
  offset += len;
  let speakerId = 0n;
  shift = 0;
  while (offset < payload.length) {
    const b = payload[offset++];
    speakerId |= BigInt(b & 0x7f) << BigInt(shift);
    if ((b & 0x80) === 0) break;
    shift += 7;
  }
  return { text, speakerId };
}

/**
 * Parses a character identifier into a u64 speaker_id bigint.
 */
export function parseSpeakerId(characterId?: string | number | bigint): bigint {
  if (typeof characterId === 'bigint') return BigInt.asUintN(64, characterId);
  if (typeof characterId === 'number') return BigInt.asUintN(64, BigInt(characterId));
  if (typeof characterId === 'string') {
    if (/^\d+$/.test(characterId)) {
      try {
        return BigInt.asUintN(64, BigInt(characterId));
      } catch {
        return 0n;
      }
    }
    const digits = characterId.replace(/\D/g, '');
    if (digits.length > 0) {
      try {
        return BigInt.asUintN(64, BigInt(digits));
      } catch {
        return 0n;
      }
    }
    return 0n;
  }
  return 0n;
}

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
 * Generates a Generast AST or Tin binary from input text and optional characterId.
 *
 * If active extensions are present, encodes `(Generast::String(text), speaker_id)` into a Tin
 * with label `LABEL_ALHALTE_GENERAST` and executes the extension pipeline.
 * Returns the resulting Tin binary (Uint8Array) or GenerastAST.
 */
export async function buildGenerastFromText(
  text: string,
  extensions: ExtensionItem[] = [],
  pipeline: ExtensionPipeline = new ExtensionPipeline(),
  characterId?: string | number | bigint,
): Promise<GenerastAST> {
  const activeExtensions = extensions.filter((ext) => ext.enabled);

  if (activeExtensions.length === 0) {
    return createTextGenerastAST(text);
  }

  try {
    const speakerId = parseSpeakerId(characterId);
    const initialBytes = encodeExtIoTin(text, speakerId);
    const resultBytes = await pipeline.executePipeline(activeExtensions, initialBytes);

    // If resultBytes is a valid Tin binary (starts with LABEL_ALHALTE_GENERAST)
    const isTin =
      resultBytes.length >= 8 &&
      new DataView(resultBytes.buffer, resultBytes.byteOffset, 8).getBigUint64(0, true) ===
        LABEL_ALHALTE_GENERAST;

    if (isTin) {
      return resultBytes;
    }

    // Fallback for mocks / plain text outputs
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
