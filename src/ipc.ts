import { invoke } from "@tauri-apps/api/core";
import type { CreateProfileResult, ProfileSummary } from "./types";

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
