export interface ProfileSummary {
  userId: string;
  displayName: string;
  createdAt: string;
}

export interface CreateProfileResult {
  summary: ProfileSummary;
  recoveryPhrase: string;
}
