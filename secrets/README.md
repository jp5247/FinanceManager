# `secrets/` — local-only, never committed

Everything in this folder (except this README) is gitignored. Use it for one-off local secrets needed during development — passwords for password-protected PDF fixtures, dev API tokens, etc. Anything dropped here stays on your machine.

## Phase-0 spike: PDF password handoff

To round-trip a password-protected savings statement through the PDF spike:

1. Create `secrets/savings-passwords.json` with one entry per file (the spike re-uses this format later for the upload UI).

   ```json
   {
     "April HDFC savings.pdf": "your-password-here",
     "May HDFC savings.pdf": "your-password-here"
   }
   ```

   If both savings PDFs share a password (typical for HDFC — derived from customer ID), the same string goes in both.

2. Tell Claude when it's in place. Claude will read the password from this file, pass it to `parse-text --password <pw>`, and confirm transactions extract from the encrypted PDF.

The file is automatically gitignored. Delete it after the spike is done if you want.
