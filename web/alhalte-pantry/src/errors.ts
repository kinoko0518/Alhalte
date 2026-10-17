/**
 * Error hierarchy for `@alhalte/pantry`.
 */

export class PantryError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'PantryError';
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class OpenerError extends PantryError {
  constructor(message: string) {
    super(message);
    this.name = 'OpenerError';
  }
}

export class UnsupportedEncryptionError extends OpenerError {
  constructor(public readonly value: number) {
    super(`Unsupported encryption mode: 0x${value.toString(16)}`);
    this.name = 'UnsupportedEncryptionError';
  }
}

export class UnsupportedCompressionError extends OpenerError {
  constructor(public readonly value: number) {
    super(`Unsupported compression algorithm: 0x${value.toString(16)}`);
    this.name = 'UnsupportedCompressionError';
  }
}

export class DecryptionError extends PantryError {
  constructor(message: string, public readonly cause?: unknown) {
    super(message);
    this.name = 'DecryptionError';
  }
}

export class MissingKeyError extends PantryError {
  constructor(message = 'Room key is required for decrypting encrypted tin') {
    super(message);
    this.name = 'MissingKeyError';
  }
}

export class MissingWrappedKeyError extends PantryError {
  constructor(message = 'Wrapped key is required for decrypting convergent tin') {
    super(message);
    this.name = 'MissingWrappedKeyError';
  }
}

export class IntegrityError extends PantryError {
  constructor(message: string) {
    super(message);
    this.name = 'IntegrityError';
  }
}

export class DecompressionError extends PantryError {
  constructor(message: string, public readonly cause?: unknown) {
    super(message);
    this.name = 'DecompressionError';
  }
}

export class CellarError extends PantryError {
  constructor(message: string, public readonly status?: number) {
    super(message);
    this.name = 'CellarError';
  }
}

export class WireFormatError extends PantryError {
  constructor(message: string) {
    super(message);
    this.name = 'WireFormatError';
  }
}
