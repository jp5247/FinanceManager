import { invoke } from "@tauri-apps/api/core";
import type { ProfileSummary } from "./types";

export const listProfiles = (): Promise<ProfileSummary[]> =>
  invoke<ProfileSummary[]>("list_profiles");

export const createProfile = (
  userId: string,
  displayName: string,
  passphrase: string,
): Promise<ProfileSummary> =>
  invoke<ProfileSummary>("create_profile", { userId, displayName, passphrase });

export const unlockProfile = (
  userId: string,
  passphrase: string,
): Promise<ProfileSummary> =>
  invoke<ProfileSummary>("unlock_profile", { userId, passphrase });

export const lockProfile = (): Promise<void> => invoke<void>("lock_profile");

export const currentProfile = (): Promise<ProfileSummary | null> =>
  invoke<ProfileSummary | null>("current_profile");
