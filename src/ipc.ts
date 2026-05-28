import { invoke } from "@tauri-apps/api/core";
import type {
  CreateProfileResult,
  DashboardData,
  FileMeta,
  LlmConfigUpdate,
  LlmConfigView,
  NewRuleSpec,
  ProfileSummary,
  RawTransaction,
  RecategorizeAllResult,
  StoredRule,
  UploadResult,
} from "./types";

export const listProfiles = (): Promise<ProfileSummary[]> =>
  invoke<ProfileSummary[]>("list_profiles");

export const createProfile = (
  userId: string,
  displayName: string,
  passphrase: string,
): Promise<CreateProfileResult> =>
  invoke<CreateProfileResult>("create_profile", { userId, displayName, passphrase });

export const unlockProfile = (
  userId: string,
  passphrase: string,
): Promise<ProfileSummary> =>
  invoke<ProfileSummary>("unlock_profile", { userId, passphrase });

export const unlockWithRecovery = (
  userId: string,
  recoveryPhrase: string,
): Promise<ProfileSummary> =>
  invoke<ProfileSummary>("unlock_with_recovery", { userId, recoveryPhrase });

export const lockProfile = (): Promise<void> => invoke<void>("lock_profile");

export const currentProfile = (): Promise<ProfileSummary | null> =>
  invoke<ProfileSummary | null>("current_profile");

export const uploadPdf = (
  filePath: string,
  password: string | null,
): Promise<UploadResult> =>
  invoke<UploadResult>("upload_pdf", { filePath, password });

export const listImports = (): Promise<FileMeta[]> =>
  invoke<FileMeta[]>("list_imports");

export const getImport = (importId: string): Promise<UploadResult> =>
  invoke<UploadResult>("get_import", { importId });

export const deleteImport = (importId: string): Promise<void> =>
  invoke<void>("delete_import", { importId });

export const recategorizeImport = (importId: string): Promise<UploadResult> =>
  invoke<UploadResult>("recategorize_import", { importId });

export const recategorizeAllImports = (): Promise<RecategorizeAllResult> =>
  invoke<RecategorizeAllResult>("recategorize_all_imports");

export const listTransactionsByCategory = (
  category: string,
): Promise<RawTransaction[]> =>
  invoke<RawTransaction[]>("list_transactions_by_category", { category });

export const listTransactionsByMonth = (
  month: string,
): Promise<RawTransaction[]> =>
  invoke<RawTransaction[]>("list_transactions_by_month", { month });

export const recategorizeTransaction = (
  importId: string,
  rowNumber: number,
  category: string,
  saveAsRule: NewRuleSpec | null = null,
): Promise<UploadResult> =>
  invoke<UploadResult>("recategorize_transaction", {
    importId,
    rowNumber,
    category,
    saveAsRule,
  });

export const listUserRules = (): Promise<StoredRule[]> =>
  invoke<StoredRule[]>("list_user_rules");

export const deleteUserRule = (ruleId: string): Promise<StoredRule[]> =>
  invoke<StoredRule[]>("delete_user_rule", { ruleId });

export const getLlmConfig = (): Promise<LlmConfigView> =>
  invoke<LlmConfigView>("get_llm_config");

export const setLlmConfig = (update: LlmConfigUpdate): Promise<LlmConfigView> =>
  invoke<LlmConfigView>("set_llm_config", { update });

export const dashboardAggregate = (): Promise<DashboardData> =>
  invoke<DashboardData>("dashboard_aggregate");
