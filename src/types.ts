export interface ProfileSummary {
  userId: string;
  displayName: string;
  createdAt: string;
}

export interface CreateProfileResult {
  summary: ProfileSummary;
  recoveryPhrase: string;
}

export interface RawTransaction {
  importId: string;
  sourceFile: string;
  sourceSha256: string;
  sourcePage: number;
  rowNumber: number;
  parserVersion: string;
  parserBackend: "pdfium" | "pdf-extract" | "ocr-tesseract";

  txnDate: string;
  description: string;
  /** Magnitude as a stringified decimal, e.g. "1582.00". `null` when not a debit row. */
  debit: string | null;
  credit: string | null;
  balance: string | null;
}

export interface FileMeta {
  importId: string;
  uploadedAt: string;
  sourceFile: string;
  sourceSha256: string;
  adapterId: string;
  adapterVersion: string;
  pageCount: number;
  transactionCount: number;
}

export interface UploadResult {
  importId: string;
  uploadedAt: string;
  sourceFile: string;
  sourceSha256: string;
  adapterId: string;
  pageCount: number;
  transactionCount: number;
  preview: RawTransaction[];
}
