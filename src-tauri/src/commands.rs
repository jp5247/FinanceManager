use crate::state::AppState;
use fm_core::UserId;
use fm_profile::{ProfileSummary, Session};
use tauri::State;

/// Returns one [`ProfileSummary`] per profile on disk. Empty list when
/// the app runs for the first time.
#[tauri::command]
pub fn list_profiles(state: State<AppState>) -> Result<Vec<ProfileSummary>, String> {
    fm_profile::list_profiles(&state.data_root).map_err(|e| e.to_string())
}

/// Create a new profile and return the resulting [`ProfileSummary`]. The
/// session is installed in [`AppState`] on success — the caller is now
/// unlocked.
#[tauri::command]
pub fn create_profile(
    user_id: String,
    display_name: String,
    passphrase: String,
    state: State<AppState>,
) -> Result<ProfileSummary, String> {
    let user = UserId::new(user_id).map_err(|e| e.to_string())?;
    let session =
        fm_profile::create_profile(&state.storage, &user, &display_name, passphrase.as_bytes())
            .map_err(|e| e.to_string())?;
    let summary = summary_from_disk(&state, &user)?;
    install_session(&state, session)?;
    Ok(summary)
}

/// Unlock an existing profile and install its session.
#[tauri::command]
pub fn unlock_profile(
    user_id: String,
    passphrase: String,
    state: State<AppState>,
) -> Result<ProfileSummary, String> {
    let user = UserId::new(user_id).map_err(|e| e.to_string())?;
    let session = fm_profile::unlock_profile(&state.storage, &user, passphrase.as_bytes())
        .map_err(|e| e.to_string())?;
    let summary = summary_from_disk(&state, &user)?;
    install_session(&state, session)?;
    Ok(summary)
}

/// Drop the current session. The KeyBytes inside zero on Drop.
#[tauri::command]
pub fn lock_profile(state: State<AppState>) -> Result<(), String> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    *guard = None;
    Ok(())
}

/// Returns the unlocked profile's summary, or `None` if locked.
#[tauri::command]
pub fn current_profile(state: State<AppState>) -> Result<Option<ProfileSummary>, String> {
    let user_id = {
        let guard = state.session.lock().map_err(|e| e.to_string())?;
        match guard.as_ref() {
            Some(s) => s.user_id().clone(),
            None => return Ok(None),
        }
    };
    summary_from_disk(&state, &user_id).map(Some)
}

fn summary_from_disk(state: &State<AppState>, user: &UserId) -> Result<ProfileSummary, String> {
    fm_profile::list_profiles(&state.data_root)
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|s| &s.user_id == user)
        .ok_or_else(|| format!("profile {user} not found after create/unlock"))
}

fn install_session(state: &State<AppState>, session: Session) -> Result<(), String> {
    let mut guard = state.session.lock().map_err(|e| e.to_string())?;
    *guard = Some(session);
    Ok(())
}
