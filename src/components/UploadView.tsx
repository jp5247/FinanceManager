import { useCallback, useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { listImports, uploadPdf } from "../ipc";
import type { FileMeta, UploadResult } from "../types";

type Stage =
  | { kind: "idle" }
  | { kind: "needsPassword"; filePath: string }
  | { kind: "uploading" }
  | { kind: "done"; result: UploadResult };

function fileNameOf(p: string): string {
  const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/"));
  return i >= 0 ? p.slice(i + 1) : p;
}

export function UploadView() {
  const [stage, setStage] = useState<Stage>({ kind: "idle" });
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [imports, setImports] = useState<FileMeta[]>([]);

  const refreshImports = useCallback(async () => {
    try {
      setImports(await listImports());
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refreshImports();
  }, [refreshImports]);

  const pickFile = async () => {
    setError(null);
    const picked = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!picked || typeof picked !== "string") return;
    await runUpload(picked, null);
  };

  const runUpload = async (filePath: string, pw: string | null) => {
    setStage({ kind: "uploading" });
    setError(null);
    try {
      const result = await uploadPdf(filePath, pw);
      setStage({ kind: "done", result });
      setPassword("");
      void refreshImports();
    } catch (e) {
      const msg = String(e);
      if (msg.toLowerCase().includes("password")) {
        setStage({ kind: "needsPassword", filePath });
        setError(msg);
      } else {
        setStage({ kind: "idle" });
        setError(msg);
      }
    }
  };

  return (
    <section className="upload-view">
      <div className="card upload-actions">
        <h2>Upload statement</h2>
        <p className="muted">
          PDF stays on this machine. Text is extracted, parsed by the issuer
          adapter, and persisted encrypted under your profile key.
        </p>

        {stage.kind === "needsPassword" ? (
          <form
            className="inline-form"
            onSubmit={(e) => {
              e.preventDefault();
              void runUpload(stage.filePath, password);
            }}
          >
            <label>
              <span>Passphrase for {fileNameOf(stage.filePath)}</span>
              <input
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                autoFocus
              />
            </label>
            <div className="row">
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => {
                  setStage({ kind: "idle" });
                  setPassword("");
                  setError(null);
                }}
              >
                Cancel
              </button>
              <button type="submit" className="btn btn-primary" disabled={!password}>
                Retry with passphrase
              </button>
            </div>
          </form>
        ) : (
          <button
            className="btn btn-primary"
            onClick={pickFile}
            disabled={stage.kind === "uploading"}
          >
            {stage.kind === "uploading" ? "Processing…" : "Choose PDF…"}
          </button>
        )}

        {error && <div className="error-text">{error}</div>}
      </div>

      {stage.kind === "done" && <ResultPanel result={stage.result} />}

      <PreviousImports imports={imports} />
    </section>
  );
}

interface ResultProps {
  result: UploadResult;
}

function ResultPanel({ result }: ResultProps) {
  return (
    <div className="card result-panel">
      <header className="result-header">
        <div>
          <h3>{result.sourceFile}</h3>
          <p className="muted small">
            {result.transactionCount} transactions · {result.pageCount} pages ·
            adapter <code>{result.adapterId}</code> · import{" "}
            <code>{result.importId}</code>
          </p>
        </div>
      </header>
      <TransactionTable rows={result.preview} />
      {result.transactionCount > result.preview.length && (
        <p className="muted small">
          Showing first {result.preview.length} of {result.transactionCount}.
        </p>
      )}
    </div>
  );
}

function TransactionTable({ rows }: { rows: UploadResult["preview"] }) {
  if (rows.length === 0) {
    return <p className="muted">No transactions parsed.</p>;
  }
  return (
    <div className="txn-table-wrapper">
      <table className="txn-table">
        <thead>
          <tr>
            <th>Date</th>
            <th>Description</th>
            <th className="num">Debit</th>
            <th className="num">Credit</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={`${r.importId}-${r.rowNumber}`}>
              <td className="mono">{r.txnDate}</td>
              <td>{r.description}</td>
              <td className="num">{r.debit ?? ""}</td>
              <td className="num credit">{r.credit ?? ""}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function PreviousImports({ imports }: { imports: FileMeta[] }) {
  if (imports.length === 0) return null;
  return (
    <div className="card previous-imports">
      <h3>Previous imports</h3>
      <ul>
        {imports.map((m) => (
          <li key={m.importId}>
            <span>{m.sourceFile}</span>
            <span className="muted small">
              {m.transactionCount} txns · {m.adapterId}@{m.adapterVersion} ·{" "}
              {m.uploadedAt}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
