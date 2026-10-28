use tauri::{utils::config::WindowConfig, Config};

/// Tauri creates configured webviews before invoking the application's setup
/// callback. Defer them so their first IPC request cannot race managed state.
pub fn defer_windows(config: &mut Config) -> Vec<WindowConfig> {
    config
        .app
        .windows
        .iter_mut()
        .filter_map(|window| {
            if !window.create {
                return None;
            }
            let deferred = window.clone();
            window.create = false;
            Some(deferred)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_app_config_cannot_load_renderers_before_managed_state() {
        let mut config: Config = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(config.app.windows.iter().filter(|w| w.create).count(), 2);
        let deferred = defer_windows(&mut config);
        // This is the predicate Tauri uses before calling app.setup.
        assert!(!config.app.windows.iter().any(|w| w.create));
        assert_eq!(deferred.len(), 2);
        let main = deferred.iter().find(|w| w.label == "main").unwrap();
        let overlay = deferred.iter().find(|w| w.label == "overlay").unwrap();
        assert!(main.visible);
        assert!(!overlay.visible);
        assert!(overlay.always_on_top);
        assert_eq!(overlay.url.to_string(), "index.html#overlay");
    }

    #[test]
    fn intentionally_manual_windows_are_not_created_and_defer_is_idempotent() {
        let mut config: Config = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        config.app.windows[1].create = false;
        let deferred = defer_windows(&mut config);
        assert_eq!(deferred.len(), 1);
        assert_eq!(deferred[0].label, "main");
        assert!(defer_windows(&mut config).is_empty());
    }
}
